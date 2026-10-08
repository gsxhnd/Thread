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
Rust 侧执行 HTTP/HTTPS 请求，Monaco 编辑请求体与只读展示响应，
环境变量 `{{variable}}` 替换、请求历史、集合/文件夹管理均可使用。
实现记录与验收对照见 [docs/08.refactor-baseline.md](docs/08.refactor-baseline.md)。

## 技术栈

Tauri v2（Rust） · React · TypeScript · shadcn/ui · Tailwind CSS · Monaco Editor · SQLite（本地数据）

## 开发

```bash
# 安装依赖（仓库使用 package-lock.json，pnpm install 亦可）
npm install

# 启动桌面开发（前端 + Rust）
npm run dev

# 仅前端调试 / 类型检查 / 前端构建
npm run web:dev
npm run typecheck
npm run web:build

# 打包
npm run build

# Rust 侧验证
cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## 文档

详见 [docs/](docs/) 目录。

## License

[MIT](LICENSE)
