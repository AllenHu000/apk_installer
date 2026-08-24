# AGENTS.md — apk_installer (iapk) 协作约定

面向 AI 助手与协作者的仓库级工作约定，跨设备复用。

## 功能增强流程（强制）

每次新增或修改功能，必须完成以下闭环，缺一不可：

1. **改代码**：遵循现有模块划分与 Rust 惯用写法。
2. **加测试**：纯函数/校验逻辑写单元测试（`#[cfg(test)]`）。
3. **同步文档**：更新 `README.md` 中受影响的部分——
   - 功能特性列表
   - 参数与选项表（务必与 `src/cli.rs` 实际定义逐项核对）
   - 使用示例
   - 执行流程
   - 模块列表（新增 `src/*.rs` 时）
4. **验证**：运行 `cargo build && cargo test`，全绿方可收尾。
5. 涉及 CLI 变更时，`cargo run -- --help` 复核帮助输出符合预期。

## 项目结构

- `src/main.rs` — 瘦入口，仅解析参数并调用 `run`
- `src/lib.rs` — 主流程编排：`run` 分发「下载模式 / 本地模式」，公共安装子流程 `connect_and_install` 复用
- `src/cli.rs` — 命令行参数定义（clap）
- `src/downloader.rs` — 文件名解析与下载
- `src/local.rs` — 本地 APK 资源包有效性校验
- `src/device.rs` — ADB 设备检测与安装

## 关键设计约束

- URL 位置参数与 `-f/--file` 互斥，二者必居其一。
- 下载模式默认安装后删除临时文件（`-k/--keep` 保留）；本地模式默认保留源文件，成功后交互询问是否清除（无 TTY 默认保留）。
- 版本降级检测/`-D` 逻辑对两种模式一致复用，位于 `device.rs` + `lib.rs::install_with_downgrade`。
- `.aab` 等非 `.apk` 格式暂不支持（TODO）。

## 提交规范

- Conventional Commits（`feat`/`fix`/`refactor`/`docs`/`chore`），英文描述意图。
- 不提交密钥、凭证、`.env`。
