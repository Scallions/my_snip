# 开发与构建

面向使用者的安装、配置和用例见 [README](../README.md)。本文保留工程运行、打包和诊断说明。

## 环境

需要 Node.js 24、npm 和 Rust stable。Windows 需要 Visual Studio 的 C++ Build Tools 与 WebView2；macOS 需要 Xcode Command Line Tools，最低系统为 macOS 13。

界面使用 Svelte 与 KaTeX，桌面核心使用 Tauri 2 / Rust。窗口按需创建，复制或取消后销毁业务 WebView，托盘保持运行。依赖版本由 `package-lock.json` 和 `src-tauri/Cargo.lock` 固定。

## 开发

```sh
npm ci
npm run desktop:dev
```

`npm run dev` 仅启动浏览器界面预览，使用开发样例数据，不具有桌面截图或真实 API 能力。正式程序通过 Rust 发起模型请求。

## 检查与打包

```sh
npm run verify
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
```

Windows：

```sh
npm run desktop:build -- --bundles nsis
```

或运行 `./scripts/build-windows.ps1`，执行检查、打包，并将版本化 EXE 和安装包复制到 `output/`。

macOS，需要在 Mac 上执行：

```sh
npm run desktop:build -- --bundles app,dmg
```

Intel 与 Apple Silicon 构建配置见 [GitHub Actions](../.github/workflows/build.yml)。CI 只生成构建产物，不创建公开 Release；当前尚未执行 Mac 构建或实机验收。没有 Apple 发布证书时采用 ad-hoc 签名，正式分发仍需签名与公证。流程参考 [Tauri 官方构建说明](https://v2.tauri.app/distribute/pipelines/github/)。

## 配置、凭据与日志

非密设置保存为 `com.mysnip.desktop/settings.json`。Windows 位于 `%APPDATA%/com.mysnip.desktop/`，Mac 位于 `~/Library/Application Support/com.mysnip.desktop/`。

密钥分别使用 Windows Credential Manager / macOS Keychain。加载设置只返回是否已保存密钥，不把旧明文回填前端。修改接口地址时，需重新填写该服务的密钥。

`logs/` 只记录阶段、受控错误码与耗时，两个轮转文件合计最多约 512 KiB；不记录接口地址、密钥、图片或公式。正常使用不落盘截图和识别结果，不包含遥测。

## 原生诊断与内存采样

```sh
cargo run --release --manifest-path src-tauri/Cargo.toml --features diagnostics,custom-protocol -- --smoke test-results/native
```

Windows 可同时运行：

```powershell
./scripts/measure-memory.ps1 -Directory ./test-results/native
```

诊断使用独立配置和随机服务名，真实截图并向本机 HTTP fixture 提交 PNG，验证系统凭据、剪贴板、默认格式和 50 次窗口清理。测试后删除自己的测试凭据，不读取用户真实 API Key。

它会打开窗口、截图并写入剪贴板，尽力恢复原文本或图片，适合在空闲时执行。截图、请求样本和报告保存在指定的 `test-results/` 目录，该目录不进入 Git。`diagnostics` feature 不编入默认发布包。

内存采样追踪本应用和后代 WebView2 进程。工作集相加可能重复计算共享页面；具体口径和已执行范围见 [验证记录](VALIDATION.md)。HTTP fixture 通过不代表真实模型准确性或 macOS 验收通过。

## 文档与截图

| 资料 | 内容 |
| --- | --- |
| [产品设计](../DESIGN.md) | 流程、主题、格式和异常恢复 |
| [系统设计](SYSTEM-DESIGN.md) | 生命周期、状态、平台适配与验收标准 |
| [Word 设计文档](My-Snip-System-Design.docx) | 系统设计文档版 |
| [选型时内存评估](Native与Tauri2内存评估.md) | 工程估算，实测以验证记录为准 |
| [界面验收](../design-qa.md) | 视觉与交互验证证据 |
| [产品截图](screenshots/) | README 使用的当前版本界面截图 |

README 截图来自 0.1.1 的实际界面，在浏览器预览中使用内置示例公式及占位接口配置。它们展示界面功能，不作为模型识别精度证据。原始视口为 1280×720、DPR 1，结果窗裁片为 820×418，设置窗为 820×592；仅裁掉窗口外背景，未改动界面文字或控件。
