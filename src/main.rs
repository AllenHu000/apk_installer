use clap::Parser;
use dialoguer::{Confirm, Select};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use url::Url;

/// APK 安装工具 - 下载并安装 APK 到 Android 设备
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// APK 下载 URL，需包含 downloadName 参数
    url: String,

    /// 输出目录，默认为当前目录
    #[arg(short, long, default_value = ".")]
    output_dir: String,

    /// 设备序列号（如果有多个设备连接）
    #[arg(short, long)]
    device: Option<String>,

    /// 强制覆盖已存在的文件，不询问
    #[arg(short, long)]
    force: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // 1. 解析 URL 并提取文件名
    let filename = extract_filename(&args.url)?;

    // 2. 确保输出目录存在
    let output_dir = Path::new(&args.output_dir);
    if !output_dir.exists() {
        fs::create_dir_all(output_dir)?;
        println!("已创建输出目录: {}", output_dir.display());
    }

    // 3. 构建完整的输出路径
    let output_path = output_dir.join(&filename);

    // 4. 检查文件是否已存在
    if output_path.exists() && !args.force {
        let confirm = Confirm::new()
            .with_prompt(format!("警告：文件 {} 已存在，是否覆盖？", output_path.display()))
            .default(false)
            .interact()?;

        if !confirm {
            println!("操作已取消");
            return Ok(());
        }
    }

    // 5. 下载文件
    download_file(&args.url, &output_path)?;

    // 6. 检查 ADB 是否已安装
    if !check_adb_installed() {
        return Err("未找到 ADB 命令，请确保 Android SDK Platform-Tools 已安装并添加到系统路径中".into());
    }

    // 7. 检查设备连接状态
    let device = if let Some(device) = args.device {
        // 用户指定了设备
        check_device_connected(&device)?;
        Some(device)
    } else {
        // 自动选择设备
        select_device()?
    };

    // 8. 安装 APK
    install_apk(&output_path, device.as_deref())?;

    println!("操作完成：{}", output_path.display());
    Ok(())
}

// 从 URL 中提取文件名
fn extract_filename(url: &str) -> Result<String, Box<dyn Error>> {
    // 尝试从 URL 查询参数中提取 downloadName
    if let Some(url) = Url::parse(url).ok() {
        if let Some(filename) = url.query_pairs().find(|(key, _)| key == "downloadName") {
            return Ok(filename.1.to_string());
        }
    }

    // 如果没有 downloadName 参数，尝试从 URL 路径中提取文件名
    let path = Path::new(url);
    if let Some(filename) = path.file_name() {
        if let Some(filename_str) = filename.to_str() {
            return Ok(filename_str.to_string());
        }
    }

    // 如果都提取失败，使用默认文件名
    Err("无法从 URL 中提取文件名，请确保 URL 包含 downloadName 参数".into())
}

// 下载文件并显示实时进度
fn download_file(url: &str, output_path: &Path) -> Result<(), Box<dyn Error>> {
    println!("下载中...: {}", url);
    println!("保存路径: {}", output_path.display());

    let client = Client::new();
    let response = client.get(url).send()?;

    // 获取总大小
    let total_size = response.content_length().unwrap_or(0);

    // 创建进度条
    let pb = if total_size > 0 {
        ProgressBar::new(total_size)
    } else {
        // 如果无法获取总大小，使用不确定的进度条
        ProgressBar::new_spinner()
    };

    // 设置进度条样式
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} ({eta})")?
            .progress_chars("#>-"),
    );

    let mut file = File::create(output_path)?;
    let mut downloaded: u64 = 0;

    // 使用真正的流式读取 - 通过 Reader trait
    use std::io::Read;
    let mut reader = response;
    let mut chunk_buffer = vec![0u8; 8192]; // 8KB 缓冲区

    loop {
        let bytes_read = reader.read(&mut chunk_buffer)?;
        if bytes_read == 0 {
            // 下载完成
            break;
        }

        let chunk = &chunk_buffer[..bytes_read];
        file.write_all(chunk)?; // 写入文件
        downloaded += bytes_read as u64; // 更新已下载字节数

        // 实时更新进度条
        if total_size > 0 {
            pb.set_position(downloaded);

            // 计算并显示速度
            if downloaded > 0 && downloaded % (1024 * 1024) == 0 { // 每1MB显示一次速度
                let elapsed = pb.elapsed().as_secs_f64();
                if elapsed > 0.0 {
                    let speed = (downloaded as f64 / 1024.0 / 1024.0) / elapsed;
                    pb.set_message(format!("{:.2} MB/s", speed));
                }
            }
        } else {
            // 对于未知大小的下载，显示当前下载大小
            pb.set_message(format!("已下载: {} MB", downloaded / 1024 / 1024));
            pb.tick();
        }
    }

    // 完成进度条
    if total_size > 0 {
        pb.finish_with_message("下载完成");
    } else {
        pb.finish_with_message("下载完成");
    }

    Ok(())
}

// 检查 ADB 是否已安装
fn check_adb_installed() -> bool {
    Command::new("which")
        .arg("adb")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

// 检查指定设备是否已连接
fn check_device_connected(device: &str) -> Result<(), Box<dyn Error>> {
    let output = Command::new("adb")
        .arg("-s")
        .arg(device)
        .arg("devices")
        .output()?;

    if !output.status.success() {
        return Err(format!("设备 {} 未连接或不可用", device).into());
    }

    Ok(())
}

// 自动选择设备
fn select_device() -> Result<Option<String>, Box<dyn Error>> {
    let output = Command::new("adb")
        .arg("devices")
        .output()?;

    if !output.status.success() {
        return Err("执行 adb devices 命令失败".into());
    }

    println!("已安装设备{}", String::from_utf8_lossy(&output.stdout));

    let output_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = output_str.lines().collect();

    if lines.len() <= 1 {
        return Err("未找到已连接的 Android 设备".into());
    }

    // 过滤出设备列表（跳过标题行和空行）
    let devices: Vec<&str> = lines[1..]
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && line.contains("device"))
        .map(|line| line.split_whitespace().next().unwrap_or(""))
        .collect();

    if devices.is_empty() {
        return Err("未找到已连接的 Android 设备".into());
    }

    if devices.len() == 1 {
        // 只有一个设备，直接使用
        println!("找到一个设备: {}", devices[0]);
        return Ok(Some(devices[0].to_string()));
    }

    // 多个设备，让用户选择
    println!("找到多个设备，请选择要安装的设备：");
    let selection = Select::new()
        .with_prompt("选择设备")
        .items(&devices)
        .default(0)
        .interact()?;

    Ok(Some(devices[selection].to_string()))
}

// 安装 APK 到设备
fn install_apk(apk_path: &Path, device: Option<&str>) -> Result<(), Box<dyn Error>> {
    println!("下载完成，正在安装：{}", apk_path.display());

    let mut cmd = Command::new("adb");

    // 如果指定了设备，添加 -s 参数
    if let Some(device_id) = device {
        cmd.arg("-s").arg(device_id);
    }

    // 添加安装命令
    cmd.arg("install").arg(apk_path);

    // 执行命令并显示输出
    let output = cmd.output()?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("安装失败：{}", error_msg).into());
    }

    let success_msg = String::from_utf8_lossy(&output.stdout);
    println!("安装成功：{}", success_msg);

    Ok(())
}
