use dialoguer::Confirm;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub mod cli;
pub mod device;
pub mod downloader;
pub mod local;

use cli::Args;

/// 解析输出目录：未显式指定时回退到系统临时目录
pub fn resolve_output_dir(output_dir: Option<&str>) -> PathBuf {
    match output_dir {
        Some(dir) => PathBuf::from(dir),
        None => std::env::temp_dir(),
    }
}

/// 程序入口：根据参数分发到「下载安装」或「本地安装」流程。
pub fn run(args: Args) -> Result<(), Box<dyn Error>> {
    match (args.url.clone(), args.file.clone()) {
        (_, Some(file)) => run_local(&args, &file),
        (Some(url), None) => run_download(&args, &url),
        // clap 的 required_unless_present 已保证二者至少有一个，此分支不可达。
        (None, None) => Err("必须提供 URL 或 --file 参数".into()),
    }
}

/// 下载安装流程：下载 APK → 选择设备 → 通过 ADB 安装。
fn run_download(args: &Args, url: &str) -> Result<(), Box<dyn Error>> {
    // 1. 解析 URL 并提取文件名
    let filename = downloader::extract_filename(url)?;

    // 2. 确保输出目录存在（未指定时使用系统临时目录）
    let output_dir = resolve_output_dir(args.output_dir.as_deref());
    if !output_dir.exists() {
        fs::create_dir_all(&output_dir)?;
        println!("已创建输出目录: {}", output_dir.display());
    }

    // 3. 构建完整的输出路径
    let output_path = output_dir.join(&filename);

    // 4. 检查文件是否已存在
    if output_path.exists() && !args.force {
        let confirm = Confirm::new()
            .with_prompt(format!(
                "警告：文件 {} 已存在，是否覆盖？",
                output_path.display()
            ))
            .default(false)
            .interact()?;

        if !confirm {
            println!("操作已取消");
            return Ok(());
        }
    }

    // 5. 下载文件
    downloader::download_file(url, &output_path)?;

    // 6. 选择设备并安装
    println!("下载完成，正在安装：{}", output_path.display());
    let installed = connect_and_install(args.device.clone(), &output_path, args.downgrade)?;

    if !installed {
        // 用户放弃降级安装：保留已下载文件便于后续重试
        println!("已取消安装，APK 已保留：{}", output_path.display());
        return Ok(());
    }

    // 7. 安装成功后清理下载的临时文件（除非用户要求保留）
    if args.keep {
        println!("已保留 APK 文件：{}", output_path.display());
    } else if let Err(e) = fs::remove_file(&output_path) {
        eprintln!("警告：清理临时文件失败 {}：{}", output_path.display(), e);
    } else {
        println!("已清理临时文件：{}", output_path.display());
    }

    println!("操作完成：{}", output_path.display());
    Ok(())
}

/// 本地安装流程：校验本地资源包有效性 → 选择设备 → 通过 ADB 安装。
fn run_local(args: &Args, file: &str) -> Result<(), Box<dyn Error>> {
    // 本地模式下无意义的参数：忽略并给出友好提示
    if args.output_dir.is_some() {
        eprintln!("提示：-o/--output-dir 在本地模式下不支持，已忽略");
    }
    if args.force {
        eprintln!("提示：--force 在本地模式下不支持，已忽略");
    }
    if args.keep {
        eprintln!("提示：-k/--keep 在本地模式下不支持（本地文件默认保留），已忽略");
    }

    // 1. 校验本地资源包有效性
    let apk_path = local::validate_local_apk(file)?;

    // 2. 选择设备并安装
    println!("正在安装本地资源包：{}", apk_path.display());
    let installed = connect_and_install(args.device.clone(), &apk_path, args.downgrade)?;

    if !installed {
        println!("已取消安装，APK 已保留：{}", apk_path.display());
        return Ok(());
    }

    // 3. 安装成功后询问是否清除（本地文件默认保留；无交互环境下默认保留）
    println!("安装成功！");
    let should_remove = Confirm::new()
        .with_prompt("遗漏的安装包会持续占用磁盘空间，是否清除？")
        .default(false)
        .interact()
        .unwrap_or(false);

    if should_remove {
        if let Err(e) = fs::remove_file(&apk_path) {
            eprintln!("警告：清除安装包失败 {}：{}", apk_path.display(), e);
        } else {
            println!("已清除安装包：{}", apk_path.display());
        }
    } else {
        println!("已保留安装包：{}", apk_path.display());
    }

    println!("操作完成：{}", apk_path.display());
    Ok(())
}

/// 公共安装子流程：检查 ADB → 选择/校验设备 → 安装（含降级处理）。
///
/// 返回 `Ok(true)` 表示已成功安装，`Ok(false)` 表示用户放弃降级安装。
fn connect_and_install(
    device: Option<String>,
    apk_path: &Path,
    downgrade: bool,
) -> Result<bool, Box<dyn Error>> {
    // 1. 检查 ADB 是否已安装
    if !device::check_adb_installed() {
        return Err(
            "未找到 ADB 命令，请确保 Android SDK Platform-Tools 已安装并添加到系统路径中".into(),
        );
    }

    // 2. 检查设备连接状态
    let device = if let Some(device) = device {
        device::check_device_connected(&device)?;
        Some(device)
    } else {
        device::select_device()?
    };

    // 3. 安装 APK（自动处理版本降级）
    install_with_downgrade(apk_path, device.as_deref(), downgrade)
}

/// 安装 APK，并在检测到版本降级时提示 / 降级重装。
///
/// 返回 `Ok(true)` 表示已成功安装，`Ok(false)` 表示用户放弃降级安装。
fn install_with_downgrade(
    apk_path: &Path,
    device: Option<&str>,
    auto_downgrade: bool,
) -> Result<bool, Box<dyn Error>> {
    use device::InstallOutcome;

    match device::install_apk(apk_path, device, auto_downgrade)? {
        InstallOutcome::Success => Ok(true),
        InstallOutcome::Failed(msg) => Err(format!("安装失败：{}", msg).into()),
        InstallOutcome::DowngradeBlocked {
            new_code,
            current_code,
        } => {
            let fmt = |c: Option<i64>| {
                c.map(|v| v.to_string())
                    .unwrap_or_else(|| "未知".to_string())
            };
            println!(
                "警告：检测到版本降级 —— 待安装包版本号 {} 低于设备已安装版本 {}",
                fmt(new_code),
                fmt(current_code)
            );

            // -D/--downgrade 已开启则自动确认，否则交互询问
            let proceed = if auto_downgrade {
                true
            } else {
                Confirm::new()
                    .with_prompt("是否降级重装（adb install -r -d，保留应用数据）？")
                    .default(false)
                    .interact()?
            };

            if !proceed {
                return Ok(false);
            }

            println!("正在降级重装...");
            match device::install_apk(apk_path, device, true)? {
                InstallOutcome::Success => Ok(true),
                InstallOutcome::Failed(msg) => Err(format!("降级重装失败：{}", msg).into()),
                InstallOutcome::DowngradeBlocked { .. } => Err("降级重装仍被系统拒绝".into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_output_dir_uses_explicit_path() {
        assert_eq!(
            resolve_output_dir(Some("/tmp/apks")),
            PathBuf::from("/tmp/apks")
        );
    }

    #[test]
    fn resolve_output_dir_falls_back_to_temp_dir() {
        assert_eq!(resolve_output_dir(None), std::env::temp_dir());
    }
}
