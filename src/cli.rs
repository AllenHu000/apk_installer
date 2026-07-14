use clap::Parser;

/// APK 安装工具 - 下载并安装 APK 到 Android 设备
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// APK 下载 URL，需包含 downloadName 参数
    pub url: String,

    /// 输出目录，默认为系统临时目录
    #[arg(short, long)]
    pub output_dir: Option<String>,

    /// 设备序列号（如果有多个设备连接）
    #[arg(short, long)]
    pub device: Option<String>,

    /// 强制覆盖已存在的文件，不询问
    #[arg(short, long)]
    pub force: bool,

    /// 安装成功后保留下载的 APK 文件（默认安装后自动删除）
    #[arg(short, long)]
    pub keep: bool,

    /// 允许降级安装：待装版本低于设备已装版本时自动降级重装（adb install -r -d），不再询问
    #[arg(short = 'D', long)]
    pub downgrade: bool,
}
