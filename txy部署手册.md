# weimo-douban txy 服务器部署手册

> 日期：2026-10-30
> 目标：将 weimo-douban 前后端部署到 txy 服务器（腾讯云 Debian 12，`212.64.24.13`，即 `~/.ssh/config` 的 `txy`，公网域名 weimo.ink），URL 分配 `/douban/`（web 前端）与 `/douban-admin/`（admin 前端）
> 参考：weimo 仓库 `docs/wiki/40-playbooks/manual-deployment-debian12.md`（手工部署模式）与 `ops/deploy/nginx/weimo-release-roots.conf`（路由与缓存契约）
> 状态：**文档先行，尚未执行任何部署**。文中所有命令均为待执行清单。

---

## 0. 快速参考（Quick Reference）

| 项 | 值 |
| --- | --- |
| web 前端 URL | `https://weimo.ink/douban/` |
| admin 前端 URL | `https://weimo.ink/douban-admin/` |
| 后端服务 | `weimo-douban.service`，监听 `127.0.0.1:8020`（仅本机，公网流量全走 nginx） |
| web API 前缀 | `/douban/api/` → `http://127.0.0.1:8020/api/` |
| admin API 前缀 | `/douban-admin/api/` → `http://127.0.0.1:8020/api/` |
| 健康检查 | `https://weimo.ink/douban/healthz` 与 `/douban-admin/healthz` → 后端 `/healthz`（返回 `{"status":"ok"}`） |
| 服务器代码 checkout | `~/workspace/weimo-douban`（root 用户） |
| 后端二进制 | `<checkout>/target/release/weimo-douban-server`（服务器上构建，x86-64） |
| 静态产物 | `/var/www/weimo-douban/douban/`（web dist）与 `/var/www/weimo-douban/douban-admin/`（admin dist）——目录名与 URL 段同构，照 weimo `/var/www/weimo/<slot>` 惯例 |
| 持久化数据 | `/srv/weimo-douban/data/`（`douban_export.db` + `covers/`），首次从本地迁移（实测约 13M） |
| 运行时配置 | `/etc/weimo-douban/douban.env`（600，root） |
| nginx 增量 | 追加进 `/etc/nginx/sites-enabled/weimo.conf` 的 443 server 块 |

与 weimo 主站的关系：

- **不并入 weimo 的签名制品管线**（`ops/deploy` 的 weimo-deploy）。该管线的 release 契约固定为「3 后端 + 4 前端」（`ops/ci/lib/components.sh` 与 `weimo_deploy/constants.py` 双端硬编码），weimo 自己的独立站点 weimo-ui 也有「不进 bundle、独立发布」的先例。weimo-douban 是独立仓库，采用 weimo playbook 中的手工部署模式（服务器 checkout 构建 + `target/release` 直跑 + `/var/www` rsync），可回滚性来自「任意 git SHA 可在服务器重建」。
- **共享**：同一台服务器、同一域名与 TLS 证书、同一个 nginx server 块（新增 location 不动既有路由）、同一套工具链。
- 若未来 douban 更新频率变高，可再评估接入制品管线或在 douban 仓库内复制一套精简 weimo-deploy（见 §10 演进）。

---

## 1. 服务器现状（2026-10-30 实测）

只读核查结果，首次部署前应复核：

| 检查项 | 实测值 | 结论 |
| --- | --- | --- |
| nginx active 配置 | `/etc/nginx/sites-enabled/weimo.conf`（含 weimo.ink 443 块） | douban location 追加到此文件 |
| 端口占用 | 8000（timu）、8001（biji）、8010（ocr-app）被占；**8020、8080 空闲** | 选定 8020 |
| 工具链 | cargo 1.94.1、pnpm 11.9.0、node v24.14.1 | 满足构建，无需另装 |
| 磁盘 | `/` 40G，已用 21G，**可用 19G** | 充足（Cargo target + 双前端 node_modules 预计 < 3G） |
| 既有 weimo 服务 | weimo-timu / weimo-biji / weimo-ocr / nginx 均 active | 部署过程不得影响 |
| douban 部署痕迹 | 无（`~/workspace` 无 weimo-douban） | 全新部署 |

构建依赖说明：后端 sqlx 使用 bundled SQLite、reqwest 使用 rustls，不需要系统 libsqlite3/libssl；weimo 部署时已装 `build-essential pkg-config`。两个前端各自有独立 `pnpm-lock.yaml`（`frontend/web`、`frontend/admin`），**均无 GitHub tarball 依赖**，服务器只需可达 npm registry（weimo 的「GitHub 不可达离线 store」问题不存在）。

---

## 2. 前置条件（Preconditions）

1. **本手册 §4 的前端改造已合入 main 并推送**。未改造的现有代码打出的包：资源路径落在 `/assets/`（撞 weimo home 站）、API 请求落在全局 `/api/`（nginx 404），必然白屏——这是硬性前置，不是可选项。
2. 服务器 8020 端口仍空闲：`ssh txy 'ss -ltn | grep 8020'` 无输出。
3. 已生成 `SYNC_SERVER_TOKEN`（见 §5.1），并决定是否配置 `SYNC_DOUBAN_COOKIE`（带 cookie 抓取可见隐藏条目；不配则免登录抓取）。
4. 本地数据迁移窗口：迁移时本地 8080 同步服务器**必须停机**（SQLite 复制一致性，见 §5.5）。
5. weimo 主站三个后端 unit 与 nginx 为 active（部署动作共用 nginx，需要 reload 窗口）：`ssh txy 'systemctl is-active weimo-timu weimo-biji weimo-ocr nginx'`。
6. 磁盘可用 ≥ 5G。

---

## 3. URL 与 nginx 路由契约

### 3.1 设计依据

- 后端路由挂在根 `/api/v1`（`routes.rs:108` `.nest("/api/v1", api)`）与 `/healthz`（不在 /api/v1 下）。因此 nginx 反代是**前缀替换**（`/douban/api/` → `/api/`），与 weimo timu/biji 的「strip 到根」不同——weimo 后端路由在根，douban 后端自带 `/api/v1`，照抄 weimo 会把请求转成 `/v1/...` 而 404。
- admin 前端的 `health()`（`frontend/admin/src/api.ts:105`）把 API base 剥掉 `/api/v1` 后拼 `/healthz`：base 为 `/douban-admin/api/v1` 时自动请求 `/douban-admin/healthz`，所以这条反代必须存在，前端**无需改动**即可工作。
- web 是 HashRouter（`App.tsx:145`），hash 部分不参与服务器路由；admin 是 BrowserRouter，**必须**配 SPA fallback。
- 缓存契约照 weimo：指纹化 `/assets/` 一年 immutable；两个 SPA 的 `index.html` 必须 `no-cache`（否则发版后 HTML 引用已删除的旧 chunk）。

### 3.2 nginx location 块（待追加到 weimo.conf 的 443 server 块内）

```nginx
# ---------- weimo-douban 开始 ----------
# API：两个前端各用自己前缀下的 /api/，替换前缀后转发（保留后端自带的 /api 前缀）
location /douban/api/ {
    proxy_pass http://127.0.0.1:8020/api/;
    include proxy_params;
    # SSE（/syncs/{id}/events）需要关闭缓冲，普通 JSON 不受影响
    proxy_buffering off;
    proxy_read_timeout 300s;
}
location /douban-admin/api/ {
    proxy_pass http://127.0.0.1:8020/api/;
    include proxy_params;
    proxy_buffering off;
    proxy_read_timeout 300s;
}

# 健康检查：admin 前端 health() 自动推导 /douban-admin/healthz；/douban/healthz 供巡检与部署验证
location = /douban/healthz {
    proxy_pass http://127.0.0.1:8020/healthz;
    include proxy_params;
}
location = /douban-admin/healthz {
    proxy_pass http://127.0.0.1:8020/healthz;
    include proxy_params;
}

# 无斜杠 301（照 weimo /timu /biji 惯例）
location = /douban { return 301 /douban/; }
location = /douban-admin { return 301 /douban-admin/; }

# 指纹化静态资源：一年 immutable
location ~ ^/(?:douban|douban-admin)/assets/ {
    root /var/www/weimo-douban;
    try_files $uri =404;
    add_header Cache-Control "public, max-age=31536000, immutable" always;
}

# SPA HTML 必须每次重验证（try_files 的 fallback 会内部重定向到这里）
location = /douban/index.html {
    root /var/www/weimo-douban;
    add_header Cache-Control "no-cache" always;
}
location = /douban-admin/index.html {
    root /var/www/weimo-douban;
    add_header Cache-Control "no-cache" always;
}

# SPA 目录（web 是 HashRouter 本不需要 fallback，配上与 admin 一致且无害）
location /douban/ {
    root /var/www/weimo-douban;
    try_files $uri $uri/ /douban/index.html;
}
location /douban-admin/ {
    root /var/www/weimo-douban;
    try_files $uri $uri/ /douban-admin/index.html;
}
# ---------- weimo-douban 结束 ----------
```

匹配顺序自检（nginx 语义）：

- `/douban/assets/x.js`：最长前缀 `/douban/` → 正则块命中 → immutable ✓
- `/douban/api/v1/...`：最长前缀 `/douban/api/`，不匹配 assets 正则 → 反代 ✓（现有全局 `location /api/ { return 404; }` 不受影响，douban API **没有**泄漏到全局 `/api/`）
- `/douban/`：前缀块 try_files → 目录索引内部重定向 → 精确块 no-cache ✓
- 与既有 `~ ^/(?:timu|biji|ocr-dashboard)/assets/`、`location /assets/`（root=home）前缀互斥，互不干扰 ✓

**注意**：不提供全局 `/api/v1` 入口（weimo 同款决策）；nginx 配置变更必须 `nginx -t` 通过后再 `systemctl reload nginx`，reload 是平滑的，不影响 weimo 三站。

---

## 4. 部署前置代码改造（先在本地完成、合入 main 后才能构建生产包）

现有代码按根路径 + Vite proxy 开发，三处不适配子路径部署。改造全部是**构建期/初始化期**配置，不改变业务逻辑，本地 `pnpm dev` 行为不变。

### 4.1 web 前端（`frontend/web`）

| # | 文件 | 现状 | 改造 |
| --- | --- | --- | --- |
| W1 | `vite.config.ts` | 无 `base` | `base: process.env.NODE_ENV === 'production' ? '/douban/' : '/'`（生产带前缀，dev 保持根路径） |
| W2 | `src/site/api.ts` 等 4 处 fetch | 硬编码 `` `/api/v1/...` `` | 引入统一常量 `const API_BASE = import.meta.env.VITE_API_BASE_URL ?? '/api/v1'`，4 处（`fetchCacheVersions`、`fetchVersionedJson`、marks 请求、demo 页）改为 `` `${API_BASE}/...` ``。dev 不传变量时回退 `/api/v1` 走 Vite proxy，与现状一致 |
| W3 | 封面/头像相对路径 | 后端返回 `/api/v1/covers?u=...` 直接赋给 `<img src>`，落在全局 `/api/` → 404 | 新增 `resolveApiUrl(path)`（照抄 `frontend/admin/src/api.ts` 的同名函数）：以 `API_BASE` 重写 `/api/v1` 前缀；`mark-card.tsx`、`douban-user-provider.tsx`、`sidebar-header.tsx` 等所有直接消费 `cover`/`avatar` 相对路径的地方统一经它转换 |
| W4 | 鉴权（若启用 token） | fetch 不带凭证，开启 `SYNC_SERVER_TOKEN` 后全部 401 | `resolveApiUrl` 内追加 `?token=`（后端 `routes.rs:131` 已为 img/EventSource 设计了 `?token=` 兜底，admin 同款用法）；fetch 请求加 `Authorization: Bearer` 头或同样走 `?token=`。token 取 `import.meta.env.VITE_API_TOKEN` |

### 4.2 admin 前端（`frontend/admin`）

| # | 文件 | 现状 | 改造 |
| --- | --- | --- | --- |
| A1 | `vite.config.ts` | 无 `base` | 同 W1，生产 `'/douban-admin/'` |
| A2 | `src/App.tsx:123` | `<BrowserRouter>` 无 basename，刷新子路由 404 | `<BrowserRouter basename={import.meta.env.BASE_URL}>`（Vite 注入 `BASE_URL`，dev 为 `/` 等价于无 basename，生产为 `/douban-admin/`） |
| A3 | `src/api.ts:23` | `getApiBase()` 默认 `'/api/v1'` | 默认值改为 `import.meta.env.VITE_API_BASE_URL ?? '/api/v1'`；localStorage 覆盖机制保留（连接弹框仍可改） |
| A4 | 鉴权 | 已完备 | 无需改造：token 输入 UI、`Authorization` 头、`resolveApiUrl` 的 `?token=` 兜底均已存在 |

### 4.3 构建命令形态（服务器与本地共用）

```bash
# web（token 为空串则省略 VITE_API_TOKEN）
cd frontend/web && pnpm install --frozen-lockfile
VITE_API_BASE_URL=/douban/api/v1 VITE_API_TOKEN=<token> pnpm build

# admin
cd frontend/admin && pnpm install --frozen-lockfile
VITE_API_BASE_URL=/douban-admin/api/v1 pnpm build
```

> token 安全语义务必认清：`VITE_API_TOKEN` 会被打进 dist 静态文件，**对任何下载 JS 的人可见**。它防的是「匿名直接 curl API 滥用写端点（创建抓取任务/清库）」，不是保密措施。web 站点本身定位就是公开可读，两者语义一致。若未来要求「公开读、鉴权写」，需后端做端点分级（见 §10）。

---

## 5. 首次部署流程（Procedure）

以下命令按序执行。`ssh txy` 均以 root 登录。

### 5.1 生成运行时密钥

本地生成一次，妥善保存（密码管理器），后续更新**不得**重新生成：

```bash
openssl rand -hex 32   # → SYNC_SERVER_TOKEN
```

### 5.2 代码上服务器（git bundle 模式，照 weimo 流程）

txy 直连 GitHub 不稳定（weimo 发布已验证此问题），用 bundle 中转。本地在 weimo-douban 仓库根、main 已推送：

```bash
git status --short                          # 必须为空
git bundle create /tmp/douban-bootstrap.bundle main
git bundle verify /tmp/douban-bootstrap.bundle
scp /tmp/douban-bootstrap.bundle txy:workspace/

ssh txy 'git clone /root/workspace/douban-bootstrap.bundle /root/workspace/weimo-douban \
         && git -C /root/workspace/weimo-douban checkout main \
         && git -C /root/workspace/weimo-douban status --short \
         && git -C /root/workspace/weimo-douban rev-parse HEAD'
# checkout 干净、HEAD == 本地 main SHA 才继续
```

### 5.3 构建后端（服务器上，x86-64）

```bash
ssh txy
cd /root/workspace/weimo-douban
cargo build --locked --release -p weimo-douban
# 产物 target/release/weimo-douban-server

# 构建后自检（照 weimo verify 惯例）
target/release/weimo-douban-server --help
file target/release/weimo-douban-server          # 必须是 ELF 64-bit LSB ... x86-64
ldd target/release/weimo-douban-server           # 不得出现 "not found"
```

### 5.4 构建前端（服务器上）

```bash
ssh txy
cd /root/workspace/weimo-douban/frontend/web
pnpm install --frozen-lockfile
VITE_API_BASE_URL=/douban/api/v1 VITE_API_TOKEN="$(grep ^SYNC_SERVER_TOKEN= /etc/weimo-douban/douban.env | cut -d= -f2)" pnpm build

cd ../admin
pnpm install --frozen-lockfile
VITE_API_BASE_URL=/douban-admin/api/v1 pnpm build
```

（env 文件在 §5.6 先建好再构建；`$(grep ...)` 方式避免 token 进 shell history。）

### 5.5 数据目录与首次数据迁移

```bash
# 服务器：建持久化目录（数据在 git checkout 之外，照 weimo 惯例）
ssh txy 'install -d -o root -g root -m 0750 /srv/weimo-douban/data'
```

本地迁移（**先停本地 8080 同步服务器**，避免复制写入中的 SQLite）：

```bash
# 本地：停服
lsof -ti tcp:8080 | xargs kill            # 确认是 weimo-douban-server 进程再杀
# 本地：迁移 db + 封面缓存（server.log 不迁）
rsync -a --progress --exclude server.log \
  /Users/jwj/workspace/weimo-douban/backend/data/ txy:/srv/weimo-douban/data/
# 本地：核对
ssh txy 'ls -la /srv/weimo-douban/data && sqlite3 /srv/weimo-douban/data/douban_export.db "PRAGMA integrity_check;"'
# integrity_check 必须返回 ok（无 sqlite3 则 apt install -y sqlite3）
```

说明：`covers/`（实测 8.5M / 273 张）是可弃缓存，不迁也能按需回源重建；迁移只为省首批回源请求。db 实测 4.1M，含全部抓取数据与手工整理内容（intro 等），**不可再生**，是迁移的核心对象。

### 5.6 运行时配置 env

```bash
ssh txy
install -d -m 0750 /etc/weimo-douban
cat > /etc/weimo-douban/douban.env <<'EOF'
RUST_LOG=weimo_douban=info,tower_http=info
SYNC_SERVER_TOKEN=<5.1 生成的 token>
# 可选：SYNC_DOUBAN_COOKIE='dbcl2="..."'
EOF
chown root:root /etc/weimo-douban/douban.env
chmod 600 /etc/weimo-douban/douban.env
```

### 5.7 systemd 服务

```bash
ssh txy
cat > /etc/systemd/system/weimo-douban.service <<'EOF'
[Unit]
Description=Weimo Douban API
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=/srv/weimo-douban
EnvironmentFile=/etc/weimo-douban/douban.env
ExecStart=/root/workspace/weimo-douban/target/release/weimo-douban-server --bind 127.0.0.1:8020 --data-dir /srv/weimo-douban/data --cors-origins https://weimo.ink
Restart=always
RestartSec=3
LimitNOFILE=65535
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now weimo-douban
systemctl status weimo-douban --no-pager
```

要点（对照 weimo unit 惯例）：`--bind 127.0.0.1`（只听本机，公网必经 nginx）；`--data-dir` 指向 checkout 外的持久化目录；`--cors-origins https://weimo.ink` 在同源反代下非必需，显式配置是为 admin 本地 dist 直连调试等场景兜底；`PrivateTmp` 已开，服务器进程写不出 /tmp 意外文件，日志走 journald（不要像本地那样落 `data/server.log`）。

### 5.8 静态产物发布

```bash
ssh txy
install -d /var/www/weimo-douban
rsync -a --delay-updates --delete-delay \
  /root/workspace/weimo-douban/frontend/web/dist/  /var/www/weimo-douban/douban/
rsync -a --delay-updates --delete-delay \
  /root/workspace/weimo-douban/frontend/admin/dist/ /var/www/weimo-douban/douban-admin/
```

`--delete-delay` 只允许用于 `/var/www/weimo-douban`，**禁止**对 `/srv/weimo-douban/data` 使用（weimo 同款红线）。

### 5.9 nginx 增量生效

把 §3.2 的 location 块追加进 `/etc/nginx/sites-enabled/weimo.conf` 的 443 server 块（建议插在现有 `/ocr/` 反代之后、现有 assets 正则之前，保持「API 块在上、静态块在下」的文件内分区）。改前备份：

```bash
ssh txy
cp /etc/nginx/sites-enabled/weimo.conf /var/backups/weimo.conf.$(date -u +%Y%m%dT%H%M%SZ)
vim /etc/nginx/sites-enabled/weimo.conf        # 追加 §3.2 内容
nginx -t                                       # 失败则回滚备份，禁止 reload
systemctl reload nginx
```

### 5.10 验证

执行 §6 全部断言。全部通过后，本地可重启 8080 恢复开发服务。

---

## 6. 验证清单（Verification）

```bash
# 1) 服务与端口
ssh txy 'systemctl is-active weimo-douban nginx'
ssh txy 'ss -ltnp | grep 8020'                 # weimo-douban-server 监听 127.0.0.1:8020

# 2) 本机后端
ssh txy 'curl -fsS http://127.0.0.1:8020/healthz'          # {"status":"ok"}

# 3) 公网健康检查与前端入口
curl -fsS  https://weimo.ink/douban/healthz                 # {"status":"ok"}
curl -fsS  https://weimo.ink/douban-admin/healthz           # {"status":"ok"}
test "$(curl -sS -o /dev/null -w '%{http_code}' -L https://weimo.ink/douban)" = 200
test "$(curl -sS -o /dev/null -w '%{http_code}' https://weimo.ink/douban-admin/)" = 200

# 4) API 经反代可用、且未泄漏到全局 /api
TOKEN=<token>
curl -fsS -H "Authorization: Bearer $TOKEN" https://weimo.ink/douban/api/v1/server/info
test "$(curl -sS -o /dev/null -w '%{http_code}' https://weimo.ink/api/v1/server/info)" = 404

# 5) 鉴权生效（若启用 token）
test "$(curl -sS -o /dev/null -w '%{http_code}' https://weimo.ink/douban/api/v1/server/info)" = 401

# 6) 缓存契约：HTML no-cache / 指纹资源 immutable + gzip
curl -fsSI https://weimo.ink/douban/ | grep -i 'cache-control'          # no-cache
curl -fsSI https://weimo.ink/douban-admin/ | grep -i 'cache-control'    # no-cache
JS=$(curl -fsS https://weimo.ink/douban/ | grep -oE '/douban/assets/[^"]+\.js' | head -1)
curl -fsS -H 'Accept-Encoding: gzip' -D - -o /dev/null "https://weimo.ink$JS" \
  | grep -iE 'content-encoding|cache-control|vary'
# 期望：gzip / immutable / Vary: Accept-Encoding

# 7) weimo 主站未受影响
for u in / /timu/ /biji/ /ocr-dashboard/; do
  test "$(curl -sS -o /dev/null -w '%{http_code}' https://weimo.ink$u)" = 200
done

# 8) 浏览器功能面（人工）
# - /douban/：书影音列表加载、封面图显示（covers 代理经 resolveApiUrl 生效）、筛选树可用
# - /douban-admin/：连接弹框确认 base=/douban-admin/api/v1、填 token 与 uid 后 Overview 显示健康；
#   创建一次小维度同步任务，观察任务流（SSE）不中断、完成后 web 端数据更新

# 9) 日志无异常
ssh txy 'journalctl -u weimo-douban -n 100 --no-pager'
```

---

## 7. 日常更新流程

适用：业务变更已在本地完成验证、单独提交并推送 main。前端产物静态同步即生效，后端变更需重启 unit。

```bash
# 本地（weimo-douban 仓库根）
git status --short && git rev-parse HEAD origin/main   # 干净且一致
git bundle create /tmp/douban-update.bundle main
git bundle verify /tmp/douban-update.bundle
scp /tmp/douban-update.bundle txy:workspace/

# 服务器：快进 + 更新前归档（回滚依据，见 §8）
ssh txy
cd /root/workspace/weimo-douban
git status --short                                        # 非空则停止
git fetch /root/workspace/douban-update.bundle main
git merge --ff-only FETCH_HEAD
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
install -d /var/backups/weimo-douban
tar -C /var/www -czf /var/backups/weimo-douban/web-$STAMP.tgz weimo-douban
cp target/release/weimo-douban-server /var/backups/weimo-douban/server-$STAMP

# 按变更范围执行（构建命令同 §5.3/§5.4）
cargo build --locked --release -p weimo-douban && systemctl restart weimo-douban
VITE_API_BASE_URL=/douban/api/v1 VITE_API_TOKEN="$(grep ^SYNC_SERVER_TOKEN= /etc/weimo-douban/douban.env | cut -d= -f2)" \
  pnpm --dir frontend/web build
VITE_API_BASE_URL=/douban-admin/api/v1 pnpm --dir frontend/admin build
rsync -a --delay-updates --delete-delay frontend/web/dist/  /var/www/weimo-douban/douban/
rsync -a --delay-updates --delete-delay frontend/admin/dist/ /var/www/weimo-douban/douban-admin/

# 验证 §6；全过后清理
rm /root/workspace/douban-update.bundle                  # 本地同步删 /tmp/douban-update.bundle
# 归档保留最近 5 份：ls -t /var/backups/weimo-douban/{web,server}-* | tail -n +11 | xargs rm -f
```

纯文档/分析类变更（不触 `backend/` 与 `frontend/`）可跳过构建与同步，只快进 checkout。

---

## 8. 回滚

- **前端**：`tar -C /var/www -xzf /var/backups/weimo-douban/web-<STAMP>.tgz` 覆盖后立即生效（无需重启任何服务）。
- **后端**：`cp /var/backups/weimo-douban/server-<STAMP> /root/workspace/weimo-douban/target/release/weimo-douban-server && systemctl restart weimo-douban`。
- **数据不回滚**：`/srv/weimo-douban/data` 与代码版本解耦；若新版本后端改了 schema 造成不可逆变更，属于 §2 前置失守，应先停服再从 §9 的备份恢复 db。
- 回滚后必须重跑 §6 验证。

---

## 9. 数据备份与运维

在线安全备份用 `sqlite3 .backup`（不停服），每日 04:17 各覆盖一周份。把下面一行写入 `/etc/cron.d/weimo-douban-backup`（cron 语法中 `%` 必须写成 `\%`，不要用 heredoc 生成以免 `$(...)` 被 shell 提前展开，直接编辑器粘贴即可）：

```cron
17 4 * * * root sqlite3 /srv/weimo-douban/data/douban_export.db ".backup /var/backups/weimo-douban/db-$(date +\%u).sqlite3"
```

服务器若无 sqlite3：`ssh txy 'apt install -y sqlite3'`。

- 日志：`journalctl -u weimo-douban -f`；同步任务记录在 db 的 records 表（保留 30 天，`SYNC_TASK_RETENTION_DAYS`）。
- 抓取行为调优（douban 独有，按需经 env 覆盖）：`SYNC_RATE_PER_MINUTE`、`SYNC_CRAWL_MIN_INTERVAL_SECS`、`SYNC_SUBJECT_*` 系列（豆瓣 rexxar 限流敏感，见《豆瓣源依赖性与脆弱性分析.md》）。
- 迁移完成后，本地 `backend/data` 不再是生产数据源；本地开发继续用本地副本，两边数据各自演化，**不要**再双向 rsync。

---

## 10. 停止条件（Stop Conditions）与演进

执行中遇到以下任一情况立即停止，不得带病继续：

- §4 前端改造未合入 main（构建出的包必然 404 白屏）。
- 8020 被未知进程占用（先查归属，不得直接杀）。
- token 未配置就公网开放（写端点匿名可达，可被滥用发起豆瓣抓取）。
- 本地 8080 未停就 rsync db；或迁移后 `integrity_check` 非 ok。
- `git bundle verify` 失败、服务器 checkout 非净、`--ff-only` 失败。
- `nginx -t` 失败（先恢复 weimo.conf 备份，绝不 reload）；weimo 三站任一验证项（§6.7）回退。
- 磁盘可用 < 5G。

演进方向（当前不做，记录决策背景）：

1. **读公开 / 写鉴权分级**：后端 token 目前是全局粒度，导致公开 web 站点必须把 token 打进前端。若要真正收紧写端点，应在后端按方法/路由分级鉴权，web 前端即可免 token。
2. **接入或复制制品管线**：更新频率上来后，可在 douban 仓库内复制 weimo-deploy 的「releases 目录 + 原子符号链接 + 激活健康检查」子集，替换本文的手工 rsync + 归档回滚。

---

## 11. 来源（Sources）

本手册的事实依据（改动相关文件时同步更新本文）：

- `backend/Cargo.toml`、`backend/src/bin/server.rs`、`backend/src/server/config.rs`（CLI 参数/环境变量）、`backend/src/server/routes.rs`（`/api/v1` 嵌挂、`/healthz`、token 中间件与 `?token=` 兜底）
- `frontend/web/vite.config.ts`、`frontend/web/src/site/api.ts`（硬编码 `/api/v1` 与封面相对路径）、`frontend/web/src/App.tsx`（HashRouter）
- `frontend/admin/vite.config.ts`、`frontend/admin/src/api.ts`（getApiBase/resolveApiUrl/health 推导）、`frontend/admin/src/App.tsx`（BrowserRouter）
- weimo 仓库：`docs/wiki/40-playbooks/manual-deployment-debian12.md`、`ops/deploy/nginx/weimo-release-roots.conf`、`ops/deploy/systemd/weimo-biji.service`、`ops/ci/adapters/txy-ci-demo.sh`（bundle/GitHub 不可达背景）
- txy 实测（2026-10-30）：`nginx -T`、`ss -ltnp`、工具链版本、磁盘、目录现状；本地 `backend/data` 体量实测
