use dialoguer::Confirm;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub mod cli;
pub mod device;
pub mod downloader;

use cli::Args;

/// 解析输出目录：未显式指定时回退到系统临时目录
pub fn resolve_output_dir(output_dir: Option<&str>) -> PathBuf {
    match output_dir {
        Some(dir) => PathBuf::from(dir),
        None => std::env::temp_dir(),
    }
}

/// 程序主流程：下载 APK 并安装到 Android 设备
pub fn run(args: Args) -> Result<(), Box<dyn Error>> {
    // 1. 解析 URL 并提取文件名
    let filename = downloader::extract_filename(&args.url)?;

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
    downloader::download_file(&args.url, &output_path)?;

    // 6. 检查 ADB 是否已安装
    if !device::check_adb_installed() {
        return Err(
            "未找到 ADB 命令，请确保 Android SDK Platform-Tools 已安装并添加到系统路径中".into(),
        );
    }

    // 7. 检查设备连接状态
    let device = if let Some(device) = args.device {
        // 用户指定了设备
        device::check_device_connected(&device)?;
        Some(device)
    } else {
        // 自动选择设备
        device::select_device()?
    };

    // 8. 安装 APK（自动处理版本降级）
    println!("下载完成，正在安装：{}", output_path.display());
    let installed = install_with_downgrade(&output_path, device.as_deref(), args.downgrade)?;

    if !installed {
        // 用户放弃降级安装：保留已下载文件便于后续重试
        println!("已取消安装，APK 已保留：{}", output_path.display());
        return Ok(());
    }

    // 9. 安装成功后清理下载的临时文件（除非用户要求保留）
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
            let fmt = |c: Option<i64>| c.map(|v| v.to_string()).unwrap_or_else(|| "未知".to_string());
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
                InstallOutcome::DowngradeBlocked { .. } => {
                    Err("降级重装仍被系统拒绝".into())
                }
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
