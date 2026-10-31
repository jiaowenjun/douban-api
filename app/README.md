# 微墨·豆瓣桌面 App（Tauri，macOS / Windows）

方案 B 全栈封装：**Tauri 壳 + 内嵌同步服务器，单进程自足运行**。不依赖
weimo.ink 也可工作；服务器数据可在首启时一键种子导入（见下）。

## 架构

```
微墨·豆瓣.app（单进程）
├─ Tauri 2（WKWebView 窗口壳，无自定义协议、无 IPC）
├─ weimo_douban::server::build_server(config)   ← backend 以 lib 依赖内嵌
│   ├─ axum API（/api/v1/**，含抓取/富集/订阅后台任务）
│   └─ rust-embed 前端 dist → 挂载在 /douban/   ← 编译期进二进制
└─ 窗口加载 http://127.0.0.1:<随机回环端口>/douban/
```

关键设计：

- **前后端同源**：本地 axum 同时服务 API 与静态资源，窗口直接开 localhost
  URL——零 CORS（无需动后端 `cors_layer`）、零 Tauri IPC、零跨进程。
- **前端零改动**：App 构建即 `pnpm build`（不注入任何环境变量）——
  `vite.config.ts` 生产 `base=/douban/` 恰好匹配挂载前缀，API 基址回落
  同源 `/api/v1`，`resolveApiUrl` 原样透传。与线上 weimo.ink/douban/ 的
  URL 结构完全一致。
- **版本原子性**：dist 经 rust-embed 编进二进制，App = 整栈快照，
  `cache_version` 协议前后端永远同版（与 txy 原子符号链接部署同构）。
- **回环随机端口**：setup 里先 `TcpListener::bind("127.0.0.1:0")` 把端口
  绑定到手（backlog 生效，后续连接不会拒绝）；仅回环监听、token 留空。
- **单实例**（tauri-plugin-single-instance）：二次启动聚焦既有窗口，
  避免两个进程写同一 SQLite。

## 桌面形态三件套（v0.4.0）

### 1. 首启种子

本地无 `douban_export.db`（从未种子、也从未同步过）时，从远端服务器拉
全库快照（后端 ≥0.3.1 的 `GET /api/v1/server/snapshot`，VACUUM INTO
一致性快照）：免去全量重抓（该规模全量抓取+富集以天计），落盘后订阅/
任务照常增量补差。covers 不随快照走，由本地封面代理按需回源自愈。
种子完成后才创建主窗口（避免窗口砸在未就绪的端口上）。

默认源 `https://weimo.ink/douban/api/v1/server/snapshot`——**生产服务器
需先部署 ≥0.3.1 后端**，否则端点 404、App 自动回落全量抓取。

### 2. 菜单栏常驻（LSUIElement）

`Info.plist` 置 `LSUIElement=true`（accessory app，无 Dock 图标）+ 状态
区托盘：**左键托盘开/聚焦主窗口，右键菜单（打开主窗口 / 检查更新… /
退出）**。关窗 = 隐藏不退出——订阅 RSS 轮询、富集流水线、缓存清理等
后台任务持续在跑（这正是「数据新鲜度 = App 存活期」的缓解手段）。
Windows 无 LSUIElement 概念（该 plist 项被忽略）：App 正常显示任务栏
图标 + 系统托盘，托盘交互/关窗隐藏行为与 macOS 一致（tauri `tray-icon`
跨平台，`main.rs` 无平台分支）。

### 3. 自动更新（tauri-plugin-updater）

- manifest 默认端点 = GitHub Releases 的 latest/download 重定向
  （仓库公开免鉴权）：`releases/latest/download/latest.json`；
- 启动 5s 后静默自检，发现新版本弹窗确认；托盘菜单可手动检查；
- minisign 签名校验（`updater.pubkey` 编入二进制；私钥在
  `~/.tauri-keys/weimo-douban-updater.key`，**丢失即无法再推送更新，
  务必备份**）；
- 安装后重启（macOS 上整包替换 .app；Windows 上装 NSIS 更新包整包替换）。

发布流程（**发版必走「完整部署」：网页端同步上 txy，见仓库根
《txy部署手册.md》§7.5**——欢迎页页脚的版本号+发布日期须与最新
Release 一致）：版本 bump 提交（四处改点：`tauri.conf.json` 与本目录
`Cargo.toml` 的 version、web 欢迎页 `APP_VERSION` 与 `APP_RELEASE_DATE`）
并 push → 手动 dispatch `app-windows` 工作流（出 Windows 产物，mac 无法
交叉编译）→ 本地 `./scripts/release.sh`（构建签名产物 → 拉取 Windows CI
产物 → 生成 latest.json → `gh release` 上传双平台 dmg/exe/tar.gz/sig/
latest.json；找不到匹配版本的 Windows 产物时自动回落仅发 macOS）→
网页端按手册 §7 部署 txy 并过 §7.5.3 一致性断言。

## Windows 版（CI 构建，mac 无法交叉编译）

同一份代码直接编 Windows：依赖面干净（reqwest 走 rustls 无 OpenSSL、
sqlx sqlite bundled、托盘用 tauri 跨平台 `tray-icon`，`main.rs` 无
平台分支），产物为 NSIS 安装包 `weimo-douban_<ver>_x64-setup.exe`
（WebView2 缺失时安装包自动引导，Win10/11 一般自带）。

构建走 GitHub Actions（`.github/workflows/app-windows.yml`，
`windows-latest`，手动 dispatch）：

1. **一次性配置 Secrets**（Settings → Secrets and variables → Actions）：
   `TAURI_SIGNING_PRIVATE_KEY` = 私钥内容
   （`cat ~/.tauri-keys/weimo-douban-updater.key`）；
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = 私钥口令（本仓密钥为空可不配）。
2. bump 版本 push 到 main → Actions 手动运行 app-windows；
3. 本地 `./scripts/release.sh` 拉取该 run 的产物（附 version.txt 校验
   版本一致才并入），与 mac 产物合并进同一 Release，latest.json 补
   `windows-x86_64` 条目（updater 直接指向 setup.exe 本体——Tauri v2
   对 exe 做 minisign 签名，无 v1 时代的 `.nsis.zip` 包装）。

CI 产物资产名全 ASCII（`weimo-douban` 前缀，与 mac 同约定——gh 上传
会剥掉 CJK 资产名前缀导致 updater URL 落空）。分发给他人的未签名 exe
首次运行有 SmartScreen 提示（「仍要运行」即可；要消除需代码签名证书，
暂不做）。

## 环境变量（从终端启动二进制时生效；Finder 启动为默认值）

| 变量 | 默认 | 说明 |
|---|---|---|
| `SYNC_SEED` | `ask` | 首启种子策略：`ask` 询问 / `auto` 直接拉 / `off` 禁用 |
| `SYNC_SEED_URL` | weimo.ink 生产端点 | 种子源（如 dev：`http://127.0.0.1:8080/api/v1/server/snapshot`） |
| `SYNC_SEED_TOKEN` | 构建期注入值 | 种子源 Bearer Token（服务器开鉴权时） |
| `SYNC_UPDATER_URL` | GitHub Releases | 更新 manifest 端点（release 构建仅接受 https；本地联调需临时开 `dangerousInsecureTransportProtocol` 重新构建） |
| `SYNC_UPDATE_AUTO` | 空 | `1` = 发现新版本不经确认直接装（无人值守） |
| `SYNC_CRAWL_*` / `SYNC_SUBJECT_*` / `SYNC_DOUBAN_COOKIE` 等 | 后端默认 | 抓取/富集调参，语义与服务器部署一致（见根 README 参数表） |

`bind`/`data_dir`/`token`/`cors_origins` 由 App 强制接管（回环、应用
数据目录、免鉴权），环境变量不生效。

## 数据目录

`~/Library/Application Support/ink.weimo.douban/`（`douban_export.db` +
`covers/`）；Windows 为 `%APPDATA%\ink.weimo.douban\`。删除该目录即回到
全新首启状态（可再次触发种子）。

手工种子（跳过下载）：

```bash
sqlite3 <源>/douban_export.db ".backup ~/Library/Application Support/ink.weimo.douban/douban_export.db"
cp -R <源>/covers ~/Library/Application Support/ink.weimo.douban/
```

## 构建与运行

```bash
# 一次性
pnpm --dir app install
# 开发（构建前端 dist + cargo run 起 App）
pnpm --dir app tauri dev
# 产出 .app / .dmg / 更新签名产物（发布用 scripts/release.sh）
pnpm --dir app tauri build
```

- 前端 dist 是编译期依赖（rust-embed，`#[folder]` 相对 Cargo.toml）：
  改前端后需重新 `tauri build`/`tauri dev`；日常前端开发仍走
  frontend/web 的 vite dev 流程。
- `cargo build` 只更新裸二进制，**不会更新 .app bundle 里的副本**——
  本地替换需手动 cp 进 `Contents/MacOS/` 或直接 `tauri build`。
- 窗口经 `run_on_main_thread` 在主线程创建（macOS 上后台线程建窗会丢
  尺寸/定位）；CGWindowList 无录屏权限时 bounds 会被涂改成 500×500，
  判断窗口状态请以进程内 tao 查询（启动日志「主窗口已创建」）为准。
- 本机自用 ad-hoc 签名即可直接运行；分发给他人的 dmg 需 Developer ID
  签名 + 公证（Apple 开发者账号）。

## 工程结构

```
app/
├── Cargo.toml          # 依赖 ../backend（weimo_douban lib）+ tauri 2（tray-icon）
├── package.json        # @tauri-apps/cli（本地 devDep，避免 dlx cwd 漂移）
├── tauri.conf.json     # 窗口在 main.rs 动态创建；updater pubkey 在 plugins 段
├── Info.plist          # 并入 bundle：LSUIElement（菜单栏常驻形态）
├── updater.pubkey      # 更新签名公钥（私钥在 ~/.tauri-keys/，勿入库）
├── icons/              # 由 frontend/web/public/favicon.svg 栅格化生成
├── scripts/release.sh  # 构建→签名→latest.json→gh release 一条龙
└── src/
    ├── main.rs         # 端口绑定/种子/服务拉起/窗口/托盘/更新/单实例
    └── static_files.rs # rust-embed dist + HashRouter 兜底 index.html
```

后端侧配套（0.3.1）：`server::build_server`（装配与绑定分离）、
`GET /api/v1/server/snapshot`（全库快照）、`server::seed`（冷启动判定 +
拉取校验落盘）。
