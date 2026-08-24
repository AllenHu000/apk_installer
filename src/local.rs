use std::error::Error;
use std::path::{Path, PathBuf};

/// 校验本地 APK 资源包的有效性。
///
/// 有效性判定规则：
/// 1. 路径必须存在且为文件；否则报「资源不存在」。
/// 2. 扩展名必须为 `.apk`（大小写不敏感）；否则报「暂不支持的安装格式」。
///    （`.aab` 等格式暂不支持，作为后续 TODO 扩展。）
/// 3. 文件大小必须大于 0；零字节通常是下载中断的半成品，报「资源无效」。
///
/// 校验通过时返回规范化后的路径。
pub fn validate_local_apk(path: &str) -> Result<PathBuf, Box<dyn Error>> {
    let p = Path::new(path);

    if !p.is_file() {
        return Err(format!("资源不存在：{}", path).into());
    }

    let is_apk = p
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("apk"))
        .unwrap_or(false);
    if !is_apk {
        // TODO: 支持 .aab / .apks / .xapk 等格式
        return Err(format!("暂不支持的安装格式：{}", path).into());
    }

    let len = std::fs::metadata(p)?.len();
    if len == 0 {
        return Err(format!("资源无效（文件为空，可能是下载中断的半成品）：{}", path).into());
    }

    Ok(p.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("apk_installer_test_{}_{}", std::process::id(), name))
    }

    #[test]
    fn rejects_missing_file() {
        let path = temp_path("missing.apk");
        let _ = std::fs::remove_file(&path);
        let err = validate_local_apk(path.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("资源不存在"));
    }

    #[test]
    fn rejects_empty_file() {
        let path = temp_path("empty.apk");
        std::fs::File::create(&path).unwrap();
        let err = validate_local_apk(path.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("资源无效"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_unsupported_extension() {
        let path = temp_path("app.aab");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"content").unwrap();
        let err = validate_local_apk(path.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("暂不支持的安装格式"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn accepts_valid_apk() {
        let path = temp_path("valid.apk");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"PK\x03\x04 fake apk content").unwrap();
        assert!(validate_local_apk(path.to_str().unwrap()).is_ok());
        let _ = std::fs::remove_file(&path);
    }
}
