# 唐三浪DBMan

> 一款基于 **Tauri 2 + Rust + Vue 3** 的现代桌面数据库管理工具，把 **MySQL / MariaDB、PostgreSQL、SQLite、SQL Server、Oracle、MongoDB、Redis** 放进同一个工作台。

**官网 / 下载地址：http://dbman.tangsanlang.com/**

安装包小、内存占用低、开箱即用，不需要额外安装运行时。

---

## 功能特性

| 特性 | 说明 |
| --- | --- |
| 多数据库一体化 | 关系型（MySQL / MariaDB、PostgreSQL、SQLite、SQL Server、Oracle）、文档型（MongoDB）、键值型（Redis）在同一个工作台里连接、切库、看结构与跑查询 |
| SSH 隧道连接 | 纯 Rust 实现的 SSH 端口转发，无需额外安装客户端，直接连内网或跳板机后面的数据库 |
| AI 生成 SQL | 用一句话描述需求，自动读取表名与字段结构后生成 SQL，支持 `@` 引用表名，过程实时回显每一步耗时 |
| 表结构设计器 | 字段、索引、外键、表选项四个页签可视化修改，实时生成 `ALTER` 语句，可手工编辑后再执行 |
| 多语句逐条执行反馈 | 一次执行多条 SQL 时，结果区按条列出「成功 / 失败 / 未执行」，附影响行数、返回行数与耗时；失败即停，未执行的语句明确标记，不会让人误以为都跑过了 |
| 表名 / 字段名智能纠错 | 从数据库报错里解析出错的表名或字段名，按相似度给出候选（最多 3 个），选定后自动回写编辑器并重跑 |
| 表列表秒开 + 按需精确行数 | 表信息一次全量加载；行数统一标注为 `information_schema` 的估算值（「约 N 行」），右键单张表「精确行数」可实时 `COUNT(*)` 拿到准数 |
| 数据导入导出 | CSV 导入可逐列关联字段并自动匹配表头（支持 GBK/GB18030）；导出支持表结构 SQL、数据 SQL 与 CSV |
| 结果集处理 | 结果表格化展示，支持查找过滤、时间戳转时间、枚举值转文字、单元格编辑（批量生成 `UPDATE`）、按列筛选生成 `WHERE`、排序分页与导出 CSV |
| 查询结果历史 | 每次执行的结果按连接留存，可随时切回历史结果回看 |
| 收藏与 SQL 日志 | 常用 SQL 一键收藏，历史执行记录可回溯 |
| 亮色 / 暗色主题 | 界面与编辑器整体换肤，偏好本地持久化 |
| 凭据安全存储 | 数据库密码、SSH 凭据、AI API Key 均存入系统钥匙串（macOS Keychain / Windows 凭据管理器 / Linux Secret Service），不落盘 |

## 支持的数据库

- **MySQL / MariaDB** —— 库表树、SQL 工作区、表结构设计器、导入导出全流程；`SHOW` / `USE` / 多语句执行均支持
- **PostgreSQL / SQL Server / Oracle** —— 经关系型驱动抽象层接入，支持库表结构浏览与 SQL 执行
- **SQLite** —— 以本地文件路径连接，无需主机与端口
- **MongoDB** —— 库 → 集合 → 文档树，查询语言为 JSON，结果区与关系型共用同一套表格
- **Redis** —— 多 DB 切换、键树浏览、值查看与编辑

## 下载

前往官网 **http://dbman.tangsanlang.com/** 或本仓库的
[Releases](../../releases) 页面下载对应平台安装包：

- **macOS**：`唐三浪DBMan_x.y.z_universal.dmg` —— 通用包，同时支持 Intel 与 Apple 芯片，要求 macOS 10.15+
- **Windows**：`唐三浪DBMan_x.y.z_x64-setup.exe`（NSIS）或 `唐三浪DBMan_x.y.z_x64_en-US.msi`
- **Linux**：`唐三浪DBMan_x.y.z_amd64.AppImage`（免安装）或 `唐三浪DBMan_x.y.z_amd64.deb`

## 技术栈

- **前端**：Vue 3（`<script setup>`）+ Pinia 2 + Vite 5 + CodeMirror 5
- **后端**：Rust + Tauri 2
  - `mysql_async` —— MySQL / MariaDB 驱动
  - `tokio-postgres` —— PostgreSQL 驱动
  - `rusqlite`（bundled）—— SQLite 驱动
  - `tiberius` —— SQL Server 驱动
  - `oracle` —— Oracle 驱动
  - `mongodb` —— MongoDB 官方驱动
  - `redis` —— Redis 驱动
  - `russh` —— 纯 Rust SSH 隧道
  - `reqwest` + `rustls` —— 大模型 HTTP 调用
  - `encoding_rs` —— GBK / GB18030 编码支持
  - `keyring` —— 密码 / 私钥 / API Key 存入系统钥匙串

除 SQLite 使用 `rusqlite` 的 bundled 版本（内置 C 版 SQLite）外，其余驱动均基于纯 Rust + rustls，
**不依赖 OpenSSL 等系统库**，因此安装包体积小、跨平台编译干净。

## 本地开发

前置要求：

- Node.js 18+
- Rust 1.88+（[rustup](https://rustup.rs/)）
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
