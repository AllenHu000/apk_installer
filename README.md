# iapk — APK 下载安装工具

一个用 Rust 编写的命令行工具，一步完成 **下载 APK → 选择设备 → 通过 ADB 安装**，并带有实时下载进度/速度显示、版本降级提示等能力。适合频繁给 Android 设备刷装测试包的场景。

## 功能特性

- 从下载 URL 自动解析文件名（优先读取 `downloadName` 查询参数，回退到 URL 路径）
- 实时进度条，显示已下载大小、**下载速度**与预计剩余时间
- 自动检测已连接设备：单设备直接使用，多设备交互选择
- 默认下载到系统临时目录，安装成功后自动清理（`--keep` 可保留）
- 下载中途失败自动删除半成品文件，不留残留
- 检测到「版本降级」时给出明确提示，确认后自动 `adb install -r -d` 重装（`-D` 可免询问）

## 环境要求

- [Rust 工具链](https://rustup.rs/)（含 `cargo`，建议 stable）
- **Android SDK Platform-Tools**，且 `adb` 已加入 `PATH`
  - macOS: `brew install --cask android-platform-tools`
  - 验证：`adb version` 与 `adb devices` 能正常输出
- 目标设备已开启「USB 调试」并授权本机

## 构建

```bash
# 克隆仓库
git clone https://github.com/AllenHu000/apk_installer.git
cd apk_installer

# 开发构建（产物在 target/debug/iapk）
cargo build

# 发布构建（体积更小、速度更快，产物在 target/release/iapk）
cargo build --release
```

## 安装

将发布版二进制拷贝到 `PATH` 中的任意目录即可，例如 `~/bin`：

```bash
cargo build --release
cp target/release/iapk ~/bin/iapk      # 确保 ~/bin 在 PATH 中
iapk --version                          # 验证：apk_installer 0.3.0
```

> 提示：裸命令 `iapk` 走 `PATH` 查找，可能指向旧的已安装版本。想确认跑的是新构建，用 `which iapk` 查看路径，或直接用 `./target/release/iapk` / `cargo run --release --` 调用。

## 使用

基本用法：

```bash
iapk <URL> [选项]
```

### 参数与选项

| 选项 | 说明 |
| --- | --- |
| `<URL>` | APK 下载地址，建议包含 `downloadName` 查询参数以确定文件名 |
| `-o, --output-dir <DIR>` | 下载输出目录，默认系统临时目录 |
| `-d, --device <SERIAL>` | 指定设备序列号（多设备时使用） |
| `-f, --force` | 文件已存在时强制覆盖，不询问 |
| `-k, --keep` | 安装成功后保留下载的 APK（默认自动删除） |
| `-D, --downgrade` | 允许降级安装：待装版本低于设备已装版本时自动 `adb install -r -d`，不再询问 |
| `-h, --help` | 查看帮助 |
| `-V, --version` | 查看版本 |

### 示例

```bash
# 最简：下载并安装到唯一连接的设备
iapk "https://cdn.example.com/app.apk?downloadName=app-release.apk"

# 指定设备（多设备场景）
iapk "https://.../app.apk?downloadName=app.apk" -d DP01D41N10065

# 指定输出目录并保留 APK 文件
iapk "https://.../app.apk?downloadName=app.apk" -o ~/Downloads --keep

# 明知是降级包，免确认直接降级重装
iapk "https://.../old.apk?downloadName=old.apk" -D
```

### 执行流程

1. 从 URL 解析文件名
2. 确保输出目录存在（默认系统临时目录）
3. 若目标文件已存在则询问是否覆盖（`-f` 跳过）
4. 下载文件并显示进度/速度
5. 检查 `adb` 是否可用
6. 选择设备（指定 / 单设备直用 / 多设备交互选择）
7. 安装 APK；若被系统以「版本降级」拒绝，提示版本号并确认后 `-r -d` 重装
8. 安装成功后清理临时文件（`--keep` 保留）

## 常见问题

- **`未找到 ADB 命令`**：安装 Android Platform-Tools 并把 `adb` 加入 `PATH`。
- **`未找到已连接的 Android 设备`**：检查数据线、USB 调试授权，`adb devices` 应能看到设备。
- **`INSTALL_FAILED_VERSION_DOWNGRADE`**：设备已装版本更高。加 `-D` 降级重装，或先 `adb uninstall <包名>` 再安装。

## 开发

```bash
cargo test                    # 运行单元测试
cargo clippy -- -D warnings   # Lint
cargo run -- --help           # 用当前源码运行
```

代码按模块组织：

- `src/main.rs` — 瘦入口，仅解析参数并调用 `run`
- `src/lib.rs` — 主流程编排（`run`）
- `src/cli.rs` — 命令行参数定义
- `src/downloader.rs` — 文件名解析与下载
- `src/device.rs` — ADB 设备检测与安装
