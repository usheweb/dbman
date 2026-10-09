# DBMan

> 一款基于 **Tauri 2 + Rust + Vue 3** 的现代桌面数据库管理工具，把 MySQL 与 Redis 放进同一个工作台。

**官网 / 下载地址：https://dbman.tangsanlang.com/**

安装包小、内存占用低、开箱即用，不需要额外安装运行时。

---

## 功能特性

| 特性 | 说明 |
| --- | --- |
| MySQL 与 Redis 一体化 | 关系型数据库与键值存储放在同一个工作台，连接、切库、看表结构、跑命令都在一处完成 |
| SSH 隧道连接 | 纯 Rust 实现的 SSH 端口转发，无需额外安装客户端，直接连内网或跳板机后面的数据库 |
| AI 生成 SQL | 用一句话描述需求，自动读取表名与字段结构后生成 SQL，过程实时回显每一步耗时 |
| 表结构设计器 | 字段、索引、外键、表选项四个页签可视化修改，实时生成 `ALTER` 语句，可手工编辑后再执行 |
| 数据导入导出 | CSV 导入可逐列关联字段并自动匹配表头（支持 GBK/GB18030）；导出支持表结构 SQL、数据 SQL 与 CSV |
| 结果集处理 | 查询结果表格化展示，支持在结果中查找过滤、时间戳转时间、枚举值转文字、导出 CSV |
| 收藏与 SQL 日志 | 常用 SQL 一键收藏，历史执行记录可回溯 |

## 下载

前往官网 **https://dbman.tangsanlang.com/** 或本仓库的
[Releases](../../releases) 页面下载对应平台安装包：

- **macOS**：`DBMan_x.y.z_universal.dmg` —— 通用包，同时支持 Intel 与 Apple 芯片，要求 macOS 10.15+
- **Windows**：`DBMan_x.y.z_x64-setup.exe`（NSIS）或 `DBMan_x.y.z_x64_en-US.msi`
- **Linux**：`DBMan_x.y.z_amd64.AppImage`（免安装）或 `DBMan_x.y.z_amd64.deb`

## 技术栈

- **前端**：Vue 3（`<script setup>`）+ Pinia 2 + Vite 5 + CodeMirror 5
- **后端**：Rust + Tauri 2
  - `mysql_async` —— MySQL 驱动
  - `redis` —— Redis 驱动
  - `russh` —— 纯 Rust SSH 隧道
  - `reqwest` + `rustls` —— 大模型 HTTP 调用
  - `encoding_rs` —— GBK/GB18030 编码支持

全部依赖均为纯 Rust 实现，**不依赖 OpenSSL 等系统库**，因此安装包体积小、跨平台编译干净。

## 本地开发

前置要求：

- Node.js 18+
- Rust stable 工具链（[rustup](https://rustup.rs/)）
- 各平台系统依赖见 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)

```bash
npm install
npm run tauri:dev      # 启动开发模式（Vite + Tauri）
```

## 打包

```bash
npm run tauri:build
```

macOS 出通用包（Intel + Apple 芯片）：

```bash
rustup target add aarch64-apple-darwin
npm run tauri:build -- --target universal-apple-darwin
```

Windows / Linux 包由 GitHub Actions 自动构建，推 `v*` 标签即可，
工作流见 [`.github/workflows/release.yml`](.github/workflows/release.yml)。

## 许可证

本项目基于 [MIT License](LICENSE) 开源。
