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

阶段 0（重构基线与存储验证）与阶段 1（HTTP MVP）已实现：工作区文件为真相（可 Git 跟踪），
Rust 侧执行 HTTP/HTTPS 请求，工作区 + 集合树 + HTTP 闭环（发送/取消/历史）可用，
环境变量 `{{variable}}` 替换、请求历史、代理控制台、CA 证书管理均可使用。

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
