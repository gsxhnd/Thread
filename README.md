# Thread / 灰线

> 草蛇灰线，伏脉千里 —— 追踪每一条请求的脉络。

**Thread** 是一款 local-first 的跨平台 API 开发与调试工具，集 **请求测试**（Postman）与 **抓包代理**（Proxyman）于一体。

## 特性

- 多协议请求测试：HTTP / gRPC / WebSocket / GraphQL / MQTT
- 系统级网络抓包代理（HTTP/HTTPS）
- 请求集合管理、环境变量、前/后置脚本
- 本地优先，数据安全可控
- 跨平台：macOS / Windows / Linux

## 当前状态

阶段 0（重构基线）与阶段 1（HTTP MVP）已实现；阶段 2 的 GraphQL、WebSocket、gRPC 和阶段 3 的显式 HTTP/HTTPS 代理也已可用（不含 MQTT 与系统代理自动配置）。工作区文件为真相（可 Git 跟踪），请求闭环、环境变量、历史、代理控制台与 CA 证书管理均可使用。

阶段 6 正在推进：cURL、Postman Collection v2.1、OpenAPI 3.x 与 HAR 的 HTTP 导入/导出已接入（OpenAPI 可导入 JSON/YAML、导出 JSON；HAR 可导入请求及导出选中的本地历史）。工作区目录级备份/恢复已接入（不含 Git 历史及本机历史/代理/密钥数据）。多平台 Release 工作流配置了 macOS `.app/.dmg` 与 `.tar.gz`、Windows `.zip`、Linux `.tar.gz` 及 SHA-256 草稿；工件未签名/公证。跨平台安装包尚未接入。完整本机数据备份、自动更新和升级兼容仍待完成。Postman 导入会清除可识别的凭据字段；HAR 可能包含敏感 Headers、Cookie 和正文，分享前应检查。

**UI 已从 Tauri + React 一次性全量重写为 [gpui-kit](https://gpui-kit.com/)（GPUI，纯 Rust）**，
Cargo workspace 分两层：

- `crates/core`（`thread-core`）：纯领域层，无 UI 依赖
- `crates/app`（`thread` 二进制）：GPUI 桌面应用

## 技术栈

Rust · gpui-kit / GPUI（桌面 UI） · tokio · reqwest · hyper（代理） · rcgen + rustls（MITM CA） · SQLite（本地运行数据）

## 开发

```bash
# 运行桌面应用（debug）
cargo run -p thread-app

# Rust 侧验证
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 打包
cargo build --release -p thread-app
```

系统依赖（GPUI 原生渲染，无需 WebView）：

- **macOS**：Xcode Command Line Tools。
- **Linux**：`libxkbcommon`、`fontconfig`、`openssl` 等（GPUI 常规依赖）。
- **Windows**：标准 MSVC 工具链。

## 文档

详见 [docs/](docs/) 目录。

## License

[MIT](LICENSE)
