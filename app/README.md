# weimo-douban macOS 桌面 App（Tauri）

方案 B 全栈封装：**Tauri 壳 + 内嵌同步服务器，单进程自足运行**，不依赖
weimo.ink（服务器数据可作首启种子导入，见下）。

## 架构

```
weimo-douban.app（单进程）
├─ Tauri 2（WKWebView 窗口壳，无自定义协议、无 IPC）
├─ weimo_douban::server::build_server(config)   ← backend 以 lib 依赖内嵌
│   ├─ axum API（/api/v1/**，含抓取/富集/订阅后台任务）
│   └─ rust-embed 前端 dist → 挂载在 /douban/   ← 编译期进二进制
└─ 窗口加载 http://127.0.0.1:<随机回环端口>/douban/
```

关键设计（对应《封装方案分析》里的结论）：

- **前后端同源**：本地 axum 同时服务 API 与静态资源，窗口直接开 localhost
  URL——零 CORS（无需动后端 `cors_layer`）、零 Tauri IPC、零跨进程。
- **前端零改动**：App 构建即 `pnpm build`（不注入任何环境变量）——
  `vite.config.ts` 生产 `base=/douban/` 恰好匹配挂载前缀，API 基址回落
  同源 `/api/v1`，`resolveApiUrl` 原样透传。与线上 weimo.ink/douban/ 的
  URL 结构完全一致。
- **版本原子性**：dist 经 rust-embed 编进二进制，App = 整栈快照，
  `cache_version` 协议前后端永远同版（与 txy 原子符号链接部署同构）。
- **回环随机端口**：Tauri setup 里先 `TcpListener::bind("127.0.0.1:0")`
  把端口绑定到手再开窗口，端口冲突免疫；仅回环监听、token 留空。
- **单实例**（tauri-plugin-single-instance）：二次启动聚焦既有窗口，
  避免两个进程写同一 SQLite。

## 数据目录

`~/Library/Application Support/ink.weimo.douban/`（`douban_export.db` +
`covers/`）。删除该目录即回到全新首启状态。

**首启种子**（可选，把服务器已温热的数据一次性搬进本地，免全量重抓）：

```bash
# 服务器侧（或本机 dev 服务）做一致性快照后拷入 App 数据目录
sqlite3 <源>/douban_export.db ".backup ~/Library/Application Support/ink.weimo.douban/douban_export.db"
cp -R <源>/covers ~/Library/Application Support/ink.weimo.douban/
```

之后 App 内同步即增量补差。

## 构建与运行

```bash
# 一次性
pnpm --dir app install
# 开发（构建前端 dist + cargo run 起 App）
pnpm --dir app tauri dev
# 产出 target/release/bundle/macos/weimo-douban.app 与 dmg
pnpm --dir app tauri build
```

- 前端 dist 是编译期依赖（rust-embed）：改前端后需重新 `tauri build`/
  `tauri dev`；日常前端开发仍走 frontend/web 的 vite dev 流程。
- 本机自用 ad-hoc 签名即可直接运行；分发给他人需 Developer ID 签名 +
  公证（Apple 开发者账号）。
- 从终端启动二进制时，`SYNC_CRAWL_*`/`SYNC_SUBJECT_*`/`SYNC_DOUBAN_COOKIE`
  等环境变量仍可覆盖抓取/富集参数（`app/src/main.rs` 用空参数 clap 解析，
  忽略 Finder 启动参数）；`bind`/`data_dir`/`token`/`cors_origins` 由 App
  强制接管。

## 工程结构

```
app/
├── Cargo.toml          # 依赖 ../backend（weimo_douban lib）+ tauri 2
├── package.json        # @tauri-apps/cli（本地 devDep，避免 dlx cwd 漂移）
├── tauri.conf.json     # 无 frontendDist/窗口配置——窗口在 main.rs 动态创建
├── icons/              # 由 frontend/web/public/favicon.svg 栅格化生成
└── src/
    ├── main.rs         # 端口绑定/服务拉起/窗口创建/单实例/失败对话框
    └── static_files.rs # rust-embed dist + HashRouter 兜底 index.html
```

后端侧配套改动：`backend/src/server/mod.rs` 拆出
`pub async fn build_server(config) -> Router`（装配与绑定分离），
`bin/server.rs` 行为不变。
