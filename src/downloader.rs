use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use std::error::Error;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use url::Url;

/// 从 URL 中提取文件名
pub fn extract_filename(url: &str) -> Result<String, Box<dyn Error>> {
    // 尝试从 URL 查询参数中提取 downloadName
    if let Ok(url) = Url::parse(url) {
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

    // 如果都提取失败，返回错误
    Err("无法从 URL 中提取文件名，请确保 URL 包含 downloadName 参数".into())
}

/// 下载文件并显示实时进度
///
/// 下载过程中若发生错误，会自动删除已写入的半成品文件，避免残留。
pub fn download_file(url: &str, output_path: &Path) -> Result<(), Box<dyn Error>> {
    match download_to_file(url, output_path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // 尽力清理半成品文件（忽略清理本身的错误）
            let _ = fs::remove_file(output_path);
            Err(e)
        }
    }
}

fn download_to_file(url: &str, output_path: &Path) -> Result<(), Box<dyn Error>> {
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

    // 设置进度条样式（含实时下载速度 bytes_per_sec）
    let style = if total_size > 0 {
        ProgressStyle::default_bar()
            .template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
            )?
            .progress_chars("#>-")
    } else {
        ProgressStyle::default_spinner()
            .template("[{elapsed_precise}] {spinner} {bytes} ({bytes_per_sec})")?
    };
    pb.set_style(style);

    let mut file = File::create(output_path)?;
    let mut downloaded: u64 = 0;

    // 使用真正的流式读取 - 通过 Reader trait
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

        // 实时更新进度（速度由进度条模板的 bytes_per_sec 自动计算）
        pb.set_position(downloaded);
    }

    // 完成进度条
    pb.finish_with_message("下载完成");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_filename_from_download_name_param() {
        let url = "https://example.com/get?downloadName=app-release.apk&x=1";
        assert_eq!(extract_filename(url).unwrap(), "app-release.apk");
    }

    #[test]
    fn extract_filename_falls_back_to_path() {
        let url = "https://example.com/files/demo.apk";
        assert_eq!(extract_filename(url).unwrap(), "demo.apk");
    }

    #[test]
    fn extract_filename_prefers_download_name_over_path() {
        let url = "https://example.com/files/ignored.apk?downloadName=real.apk";
        assert_eq!(extract_filename(url).unwrap(), "real.apk");
    }

    #[test]
    fn extract_filename_errors_when_unresolvable() {
        // 空字符串既无 downloadName 参数，也无可用路径段
        assert!(extract_filename("").is_err());
    }
}
