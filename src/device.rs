use dialoguer::Select;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::error::Error;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

/// 在 Windows 上可执行文件为 `adb.exe`，其他平台为 `adb`。
fn adb_executable_name() -> &'static str {
    if cfg!(windows) {
        "adb.exe"
    } else {
        "adb"
    }
}

/// 解析 adb 可执行文件路径。
///
/// Windows 下 `Command::new("adb")` 依赖的 PATH 查找不可靠（cmd 能找到但进程
/// 创建时未命中），因此显式按以下顺序解析出完整路径：
/// 1. 环境变量 `ANDROID_HOME` / `ANDROID_SDK_ROOT` 下的 `platform-tools`
/// 2. `PATH` 中的各目录（按平台追加可执行文件扩展名）
/// 3. 回退到裸命令名，交给系统解析
fn find_adb() -> String {
    let exe = adb_executable_name();

    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(dir) = env::var_os(var) {
            let candidate = Path::new(&dir).join("platform-tools").join(exe);
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }

    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path) {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }

    exe.to_string()
}

/// 返回缓存的 adb 可执行文件路径。
fn adb_path() -> &'static str {
    static ADB: OnceLock<String> = OnceLock::new();
    ADB.get_or_init(find_adb).as_str()
}

/// 检查 ADB 是否已安装
///
/// 直接尝试执行 `adb --version`，跨平台可用（避免 which/where 差异）。
pub fn check_adb_installed() -> bool {
    Command::new(adb_path())
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// 检查指定设备是否已连接
pub fn check_device_connected(device: &str) -> Result<(), Box<dyn Error>> {
    let output = Command::new(adb_path())
        .arg("-s")
        .arg(device)
        .arg("devices")
        .output()?;

    if !output.status.success() {
        return Err(format!("设备 {} 未连接或不可用", device).into());
    }

    Ok(())
}

/// 从 `adb devices` 的输出中解析出在线设备序列号列表
pub fn parse_device_list(adb_output: &str) -> Vec<String> {
    let lines: Vec<&str> = adb_output.lines().collect();

    if lines.len() <= 1 {
        return Vec::new();
    }

    // 过滤出设备列表（跳过标题行和空行，仅保留状态为 device 的行）
    lines[1..]
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && line.contains("device"))
        .filter_map(|line| line.split_whitespace().next())
        .map(|serial| serial.to_string())
        .collect()
}

/// 自动选择设备
pub fn select_device() -> Result<Option<String>, Box<dyn Error>> {
    let output = Command::new(adb_path()).arg("devices").output()?;

    if !output.status.success() {
        return Err("执行 adb devices 命令失败".into());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    println!("已安装设备{}", output_str);

    let devices = parse_device_list(&output_str);

    if devices.is_empty() {
        return Err("未找到已连接的 Android 设备".into());
    }

    if devices.len() == 1 {
        // 只有一个设备，直接使用
        println!("找到一个设备: {}", devices[0]);
        return Ok(Some(devices[0].clone()));
    }

    // 多个设备，让用户选择
    println!("找到多个设备，请选择要安装的设备：");
    let selection = Select::new()
        .with_prompt("选择设备")
        .items(&devices)
        .default(0)
        .interact()?;

    Ok(Some(devices[selection].clone()))
}

/// 安装 APK 的结果
#[derive(Debug)]
pub enum InstallOutcome {
    /// 安装成功
    Success,
    /// 因版本降级被系统拒绝，携带解析出的（待装版本号，设备已装版本号）
    DowngradeBlocked {
        new_code: Option<i64>,
        current_code: Option<i64>,
    },
    /// 其他原因失败，携带原始 stderr
    Failed(String),
}

/// 判断 adb 的 stderr 是否为版本降级失败
pub fn is_downgrade_error(stderr: &str) -> bool {
    stderr.contains("INSTALL_FAILED_VERSION_DOWNGRADE")
}

/// 从降级失败信息中解析出（待装版本号，设备已装版本号）
///
/// 典型信息："Update version code 78399321 is older than current 78400941"
pub fn parse_downgrade_versions(stderr: &str) -> (Option<i64>, Option<i64>) {
    fn number_after(text: &str, marker: &str) -> Option<i64> {
        let start = text.find(marker)? + marker.len();
        let digits: String = text[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    }

    let new_code = number_after(stderr, "version code ");
    let current_code = number_after(stderr, "current ");
    (new_code, current_code)
}

/// 安装 APK 到设备。
///
/// `allow_downgrade` 为 true 时附加 `-r -d`，允许覆盖安装且允许版本降级。
pub fn install_apk(
    apk_path: &Path,
    device: Option<&str>,
    allow_downgrade: bool,
) -> Result<InstallOutcome, Box<dyn Error>> {
    let mut cmd = Command::new(adb_path());

    // 如果指定了设备，添加 -s 参数
    if let Some(device_id) = device {
        cmd.arg("-s").arg(device_id);
    }

    // 添加安装命令；降级模式下附加 -r（重装）-d（允许降级）
    cmd.arg("install");
    if allow_downgrade {
        cmd.arg("-r").arg("-d");
    }
    cmd.arg(apk_path);

    // 安装期间 adb 会阻塞数秒且无输出，用 spinner 提供实时反馈
    let spinner = ProgressBar::new_spinner();
    if let Ok(style) =
        ProgressStyle::default_spinner().template("{spinner:.cyan} [{elapsed_precise}] {msg}")
    {
        spinner.set_style(style);
    }
    spinner.set_message("安装中…");
    spinner.enable_steady_tick(Duration::from_millis(100));

    // 执行命令
    let output = cmd.output()?;

    if output.status.success() {
        spinner.finish_and_clear();
        let success_msg = String::from_utf8_lossy(&output.stdout);
        println!("安装成功：{}", success_msg);
        return Ok(InstallOutcome::Success);
    }

    spinner.finish_and_clear();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if is_downgrade_error(&stderr) {
        let (new_code, current_code) = parse_downgrade_versions(&stderr);
        Ok(InstallOutcome::DowngradeBlocked {
            new_code,
            current_code,
        })
    } else {
        Ok(InstallOutcome::Failed(stderr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_device_list_empty_output() {
        let output = "List of devices attached\n";
        assert!(parse_device_list(output).is_empty());
    }

    #[test]
    fn parse_device_list_single_device() {
        let output = "List of devices attached\nemulator-5554\tdevice\n";
        assert_eq!(parse_device_list(output), vec!["emulator-5554"]);
    }

    #[test]
    fn parse_device_list_multiple_devices() {
        let output = "List of devices attached\nemulator-5554\tdevice\nABC123XYZ\tdevice\n";
        assert_eq!(
            parse_device_list(output),
            vec!["emulator-5554", "ABC123XYZ"]
        );
    }

    #[test]
    fn parse_device_list_skips_offline_and_unauthorized() {
        let output = "List of devices attached\nemulator-5554\tdevice\ndead0001\toffline\ndead0002\tunauthorized\n";
        assert_eq!(parse_device_list(output), vec!["emulator-5554"]);
    }

    #[test]
    fn is_downgrade_error_detects_marker() {
        let stderr = "adb: failed to install app.apk: Failure [INSTALL_FAILED_VERSION_DOWNGRADE: Downgrade detected]";
        assert!(is_downgrade_error(stderr));
        assert!(!is_downgrade_error(
            "adb: failed to install app.apk: Failure [INSTALL_FAILED_INVALID_APK]"
        ));
    }

    #[test]
    fn parse_downgrade_versions_extracts_both_codes() {
        let stderr = "adb: failed to install app.apk: Failure [INSTALL_FAILED_VERSION_DOWNGRADE: Downgrade detected: Update version code 78399321 is older than current 78400941]";
        assert_eq!(
            parse_downgrade_versions(stderr),
            (Some(78399321), Some(78400941))
        );
    }

    #[test]
    fn parse_downgrade_versions_returns_none_when_absent() {
        assert_eq!(
            parse_downgrade_versions("some unrelated error"),
            (None, None)
        );
    }
}
