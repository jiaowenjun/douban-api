# 豆瓣数据同步服务器 API 设计（v1）

> 状态：**已实现**（v0.2.0，含 SSE 推流、页级进度、16 个 oneshot 集成测试）
> 日期：2026-10-27
> 前置阅读：[豆瓣API调研报告.md](豆瓣API调研报告.md)（路径②：免登录公开页抓取）、[README.md](README.md)（现有 CLI 能力）

---

## 目录

- [0. 目标与约束](#0-目标与约束)
- [1. 总体架构](#1-总体架构)
- [2. 核心设计决策与理由](#2-核心设计决策与理由)
- [3. API 一览](#3-api-一览)
- [4. 接口详细设计](#4-接口详细设计)
- [5. 数据模型](#5-数据模型)
- [6. 任务状态机](#6-任务状态机)
- [7. 缓存与数据保留策略](#7-缓存与数据保留策略)
- [8. 限速与并发控制](#8-限速与并发控制)
- [9. 错误响应规范](#9-错误响应规范)
- [10. 认证与安全](#10-认证与安全)
- [11. 其他横切关注点](#11-其他横切关注点)
- [12. 非目标](#12-非目标)
- [13. 开放问题](#13-开放问题)

---

## 0. 目标与约束

**免登录同步任意豆瓣用户的公开书影音标记与短评入库**，对外提供查询 API。

三条硬约束决定了 API 形态：

| 约束 | 来源 | 对设计的直接影响 |
|---|---|---|
| 抓取慢：全量 3 类别约 20 页 × ≥2s 限速 ≈ 1 分钟 | 豆瓣反爬要求限速 | 不能做同步接口，必须**异步任务** |
| 服务端单 IP 出口 | 服务器代替所有客户端访问豆瓣 | 必须**全局限速 + 缓存 + 单用户单飞**，否则 IP 必被封 |
| 数据是公开只读快照 | 免登录路径的能力边界 | 只做**读**服务，无写操作、无登录态、无私密数据 |

## 1. 总体架构

```
客户端 ──HTTP──▶ API 层（axum，未来实现）
                  │  创建/查询/删除任务，下载结果，读缓存数据
                  ▼
             任务管理器（内存队列 + SQLite 持久化）
                  │  single-flight、状态机、进度聚合
                  ▼
             抓取器（复用现有 parse_items / fetch_html 逻辑）
                  │  全局令牌桶（≥2s/请求）· 同 uid 串行 · 熔断
                  ▼
             豆瓣公开收藏页（movie/book/music.douban.com）
                  
             存储层：SQLite（任务调度状态 + records 数据表，抓取结果直接入库）
```

组件职责：

- **API 层**：参数校验、鉴权（可选）、客户端限流、路由
- **任务管理器**：任务队列与状态机；对同一 `uid+category+status` 维度做 single-flight（去重合并）
- **抓取器**：抓取/解析/重试/断点逻辑（含 Referer、两代页面结构兼容）
- **存储**：SQLite 单库——任务/维度调度表 + `records` 正规化数据表（Record 12 字段一行一条，按维度长期保留；不生成导出文件，数据经 API 查询）

## 2. 核心设计决策与理由

### D1：异步任务模型（而非同步等待）

全量抓取 ≥1 分钟，超出任何合理的 HTTP 超时。`POST /syncs` 创建任务后立即返回 `202`，客户端轮询（或 SSE）拿进度，完成后经 `/users/{uid}/marks` 浏览库内数据。

对极小数据量提供**有限同步等待**逃生门：`?wait_timeout=N`（N ≤ 30s），超时未完成仍返回任务 ID，不阻塞。

### D2：按维度缓存，任务结果可直接命中

缓存键 = `(uid, category, status)`，TTL 默认 24h。同参数请求在缓存有效期内**秒回**（不碰豆瓣）。`refresh: true` 强制重抓。

理由：书影音标记变化频率低（天级），而每次回源成本高（多页 × 限速）且消耗服务器 IP 信誉。

### D3：single-flight——同维度并发请求合并

同一 `(uid, category, status)` 已有运行中任务时，`POST /syncs` 不新建任务，直接返回已有 task（幂等）。防止多个客户端同时触发对同一用户的重复抓取。

### D4：豆瓣侧全局节流是服务生存的前提

所有抓取经过**单一全局令牌桶**（默认 ≥2s/请求）+ 同 uid 串行 + 连续失败熔断（详见第 8 节）。对外服务再大的并发，到豆瓣侧也恒定为低速串行流。这也是"集中式同步服务"的核心价值：共享缓存、保护出口 IP。

## 3. API 一览

Base URL：`http://{host}:8080/api/v1`（默认端口 8080，可配置）

| # | 方法 | 路径 | 用途 |
|---|---|---|---|
| 1 | POST | `/syncs` | 创建同步任务（核心入口） |
| 2 | GET | `/syncs/{task_id}` | 查询任务状态与进度 |
| 3 | GET | `/syncs` | 任务列表（过滤、游标分页；默认不含 auto 任务） |
| 4 | DELETE | `/syncs/{task_id}` | 删除任务记录（数据保留） |
| 5 | GET | `/syncs/{task_id}/result` | 结果元数据（维度汇总） |
| 6 | GET | `/users/{uid}/marks` | 读库内数据（JSON，分页） |
| 7 | GET | `/syncs/{task_id}/events` | 任务进度 SSE 推流（支持 `?token=` 兜底鉴权） |
| 8 | GET | `/server/info` | 服务元信息与限速/缓存配置 |
| 9 | GET | `/healthz` | 健康检查（在 `/api/v1` 之外） |
| 10 | GET | `/subscriptions` | 订阅列表 |
| 11 | PUT | `/subscriptions/{uid}` | 创建/更新订阅（RSS 增量 + 定时全量） |
| 12 | DELETE | `/subscriptions/{uid}` | 取消订阅 |

## 4. 接口详细设计

### 4.1 创建同步任务

```
POST /api/v1/syncs
Content-Type: application/json
```

请求体：

```json
{
  "user": "wenjun30",
  "categories": ["movie", "book", "music"],
  "statuses": ["collect"],
  "refresh": false,
  "wait_timeout": 0
}
```

| 字段 | 类型 | 默认 | 校验 |
|---|---|---|---|
| `user` | string | 必填 | 豆瓣用户 ID 或 `/people/{id}/` URL；ID 规则（`[A-Za-z0-9._-]+`） |
| `categories` | string[] | `["movie","book","music"]` | 枚举 |
| `statuses` | string[] | `["collect"]` | `collect` / `do` / `wish` |
| `refresh` | bool | `false` | `true` 时忽略未过期缓存，强制回源（清空该维度旧数据后重抓） |
| `wait_timeout` | int(秒) | `0` | `0`–`30`；>0 时服务端最多等待该时长，完成即返回 `completed` |

> 注：数据直接入 SQLite，不再有 `formats` 参数（传入未知字段将返回 400）。

响应 `202 Accepted`：

```json
{
  "task_id": "sync_01JD6Z8W4R9TQK5M2VX3YB7C8D",
  "user": "wenjun30",
  "status": "queued",
  "cached_dimensions": 1,
  "created_dimensions": 2,
  "created_at": "2026-10-27T10:00:00+08:00",
  "url": "/api/v1/syncs/sync_01JD6Z8W4R9TQK5M2VX3YB7C8D"
}
```

行为规则：

- 请求的每个 `(category, status)` 维度独立判定：缓存有效 → 直接标记完成；运行中任务存在 → 合并引用；否则入队
- 全部维度命中缓存且 `refresh=false` → 直接返回 `status: "completed"`（等价 4.2 的终态响应）
- 触发 `wait_timeout` 且在时限内完成 → 返回终态响应
- `cached_dimensions + created_dimensions` 说明各维度来源

错误：`400 INVALID_ARGUMENT`（非法 uid / 枚举值）；`429 TOO_MANY_TASKS`（运行中任务数达上限）。

### 4.2 查询任务

```
GET /api/v1/syncs/{task_id}
```

响应 `200`：

```json
{
  "task_id": "sync_01JD6Z8W4R9TQK5M2VX3YB7C8D",
  "user": "wenjun30",
  "status": "running",
  "request": { "categories": ["movie","book","music"], "statuses": ["collect"],
               "refresh": false },
  "progress": [
    { "category": "movie", "status": "collect", "state": "running",
      "fetched": 45, "total": 209, "percent": 21,
      "next_start": 45, "page": 3, "pages_total": 14 },
    { "category": "book",  "status": "collect", "state": "done",
      "fetched": 55, "total": 55, "percent": 100,
      "next_start": 60, "page": 4, "pages_total": 4 },
    { "category": "music", "status": "collect", "state": "queued",
      "fetched": 0,  "total": null, "percent": null,
      "next_start": 0, "page": 0, "pages_total": null }
  ],
  "items_count": 100,
  "error": null,
  "percent": 38,
  "created_at": "2026-10-27T10:00:00+08:00",
  "finished_at": null,
  "result_url": "/api/v1/syncs/sync_01JD6Z8W4R9TQK5M2VX3YB7C8D/result",
  "expires_at": "2026-11-03T10:00:00+08:00"
}
```

说明：

- `total` 来自豆瓣页面的 `subject-num`，首页抓到前为 `null`
- `fetched` 为**已去重**条目数；`total` 与最终 `fetched` 差 1~2 属正常（豆瓣隐藏已删条目，见 README 已知限制）
- `items_count` 为各完成维度累计条数
- **进度百分比**（服务端直接算好，前端免计算）：
  - 维度级 `progress[].percent`：`done` 恒为 `100`；`error` 或总数未知（`total` 为 `null`）时为 `null`；进行中按 `fetched/total` 向下取整，**封顶 99**（避免显示 100 而实际未完成）
  - 任务级 `percent`：`completed` 恒为 `100`；否则对 `total` 已知的维度求 `Σfetched/Σtotal` 向下取整并封顶 99；全部维度总数未知时为 `null`。注意：未抓到首页的维度不计入分母，某维度总数揭晓时百分比可能回落（如 51% → 35%），属正常现象
- **页级进度**：`next_start`（下一个待抓分页偏移，= 已抓页数 × 15）、`page`（= next_start/15，0 表示未开始）、`pages_total`（由 total 向上取整，未知为 null）——条数百分比在两次翻页之间（约 2~3s）静止，前端可用"第 {page+1}/{pages_total} 页"提供持续反馈
- 错误信息（如有）：

```json
"error": {
  "code": "DOUBAN_BLOCKED",
  "message": "movie 维度连续 5 次重试后仍 403",
  "failed_dimensions": [{"category": "movie", "status": "collect"}]
}
```

错误：`404 TASK_NOT_FOUND`。

### 4.3 任务列表

```
GET /api/v1/syncs?user=wenjun30&status=running&limit=20&cursor=sync_01JD...
```

| 参数 | 说明 |
|---|---|---|
| `user` | 按用户过滤 |
| `status` | `queued` / `running` / `completed` / `partial` / `failed` |
| `source` | 任务来源：`manual`（默认）/ `auto`（订阅全量同步）/ `all`。默认只返回手动任务，auto 任务不占列表注意力，详情仍可按 task_id 直查 |
| `limit` | 默认 20，上限 100 |
| `cursor` | 游标（上一页最后一条 task_id），按创建时间倒序 |

响应 `200`：

```json
{
  "tasks": [ { ...同 4.2 摘要字段... } ],
  "next_cursor": "sync_01JD6A...",
  "has_more": true
}
```

### 4.4 删除任务

```
DELETE /api/v1/syncs/{task_id}
```

- `204 No Content`：删除任务记录（任务/维度关联/task_dims 行）
- **不删除 records 数据**（数据属于维度级共享资源，与任务生命周期解耦，见 D2/D3）
- `404 TASK_NOT_FOUND`
- 运行中任务允许删除：先取消再清理（`state: running` 的维度中断，断点保留在其缓存记录中）

### 4.5 结果元数据

```
GET /api/v1/syncs/{task_id}/result
```

响应 `200`：

```json
{
  "task_id": "sync_01JD6Z8W4R9TQK5M2VX3YB7C8D",
  "user": "wenjun30",
  "summary": {
    "total": 291,
    "with_comment": 272,
    "by_category_status": { "movie·看过": 208, "book·读过": 55, "music·听过": 28 }
  },
  "dimensions": [
    { "category": "movie", "status": "collect", "items": 208, "with_comment": 200 },
    { "category": "book",  "status": "collect", "items": 55,  "with_comment": 50 }
  ],
  "expires_at": "2026-11-26T10:00:00+08:00"
}
```

`dimensions` 为各成功维度的汇总；数据本体经 4.6 的 `/users/{uid}/marks` 分页浏览。

`status` 非终态时返回 `409 TASK_NOT_FINISHED`（附当前 status 与 progress 摘要）。

### 4.6 读库内数据

```
GET /api/v1/users/{uid}/marks?category=movie&status=collect&page=1&page_size=50
```

定位：**只读库内数据**（`records` 表，SQL 分页、按日期倒序），给应用用的结构化数据接口。不触发抓取。

响应 `200`：

```json
{
  "user": "wenjun30",
  "category": "movie",
  "status": "collect",
  "cached_at": "2026-10-27T09:30:00+08:00",
  "fresh": true,
  "total": 208,
  "page": 1,
  "page_size": 50,
  "items": [ { ...Record 的 12 字段... } ]
}
```

| 参数 | 说明 |
|---|---|
| `category` / `status` | 必填，单维度查询 |
| `page` / `page_size` | 默认 1 / 50，page_size 上限 200 |
| `fresh` | `cached_at` 是否仍在 TTL 内 |

缓存未命中：`404 DIMENSION_NOT_CACHED`，响应体附引导：

```json
{ "error": { "code": "DIMENSION_NOT_CACHED",
  "message": "该维度无缓存，请先创建同步任务",
  "hint": "POST /api/v1/syncs {\"user\":\"wenjun30\",\"categories\":[\"movie\"],\"statuses\":[\"collect\"]}" } }
```

### 4.7 任务进度 SSE（已实现）

```
GET /api/v1/syncs/{task_id}/events
Accept: text/event-stream
```

事件格式（与 4.2 的 progress 同构，轮询的替代方案；连接建立即推一次当前快照）：

```
event: progress
data: {"task_id":"sync_...","status":"running","progress":[...],"percent":38,"items_count":100}

event: done
data: {"task_id":"sync_...","status":"completed","result_url":"..."}

event: error
data: {"task_id":"sync_...","status":"failed","error":{...}}
```

- 连接建立时先推当前快照（`progress`），此后仅在进度变更时推送（worker 经进程内 broadcast 通道通知，无轮询开销）
- 任务进入终态后发送 `done`（`partial` 也归 `done`，数据里带 error 字段）/ `error`（仅 `failed`）并关闭流
- 带 15s keep-alive 心跳；客户端断开即释放
- **鉴权注意**：浏览器 `EventSource` 无法设置自定义 Header，启用 token 时本端点支持 `?token=<token>` 查询参数兜底（其余端点仍要求 Authorization 头）
- data 为完整的 4.2 任务视图 JSON（含 percent/page 等全部字段）

### 4.8 服务元信息

```
GET /api/v1/server/info
```

```json
{
  "name": "douban-sync-server",
  "version": "0.3.0",
  "crawl": { "min_interval_seconds": 2.0, "max_concurrent_users": 2,
             "circuit_breaker": { "threshold": 5, "cooldown_seconds": 600 } },
  "cache": { "ttl_seconds": 86400, "retention_task_days": 30 },
  "queue": { "running": 1, "queued": 0 }
}
```

### 4.9 健康检查

```
GET /healthz    → 200 {"status":"ok"}
```

不在 `/api/v1` 前缀下、不鉴权，供容器探活。

### 4.10 订阅同步（RSS 增量 + 定时全量）

**模型**：未订阅用户维持现状——手动建任务触发重抓。订阅用户由后端两个周期自动同步：

- **短周期（RSS 增量）**：轮询豆瓣官方动态 feed `www.douban.com/feed/people/{uid}/interests`（免登录、仅最新约 10 条、条目 guid 尾段与页面 `data-cid` 同一 ID 空间）。条目解析入库沿用 `INSERT OR IGNORE`：已存在（主键冲突）忽略，新标记插入。**只写本地已有维度**——订阅只维护既有维度，新维度由用户手动建任务引入。RSS 修改/删除不可感知，由长周期全量兜底收敛。
- **长周期（定时全量）**：为该 uid 自动创建 `source='auto'` 的任务，对其全部 `ok/error` 维度执行 refresh 语义（DELETE + 全量重抓）。auto 任务不进默认任务列表（`?source=auto` 或按 task_id 可查），SSE/终态/保留期与手动任务一致。

**调度**：独立循环与 worker 同构（30s 空闲兜底扫描，每次处理一个到期订阅）；RSS 失败按 1/2/…分钟退避（封顶 15 分钟）不熔断主流程——RSS 是可失败的增强层，最坏退化为纯长周期。全量创建前做队列上限与 single-flight（该 uid 已有未终态 auto 任务则顺延 5 分钟）检查；`queued/running` 维度跳过本次（避免与进行中抓取竞态删数据）。

```
PUT /api/v1/subscriptions/{uid}
    body: {"rss_interval_secs": 600, "full_interval_secs": 86400}   # 两者均可省略取默认
GET  /api/v1/subscriptions
DELETE /api/v1/subscriptions/{uid}
```

约束：`rss_interval_secs ≥ 60`、`full_interval_secs ≥ 3600` 且 `full ≥ rss`（违反返回 400）。重复 PUT 只改间隔并重新启用，不重置既有节奏。默认值由服务端配置 `SYNC_SUB_RSS_INTERVAL_SECS`（600）/ `SYNC_SUB_FULL_INTERVAL_SECS`（86400）决定。

PUT 响应 `201` / GET 响应 `200`：

```json
{
  "uid": "wenjun30",
  "enabled": true,
  "rss_interval_secs": 600,
  "full_interval_secs": 86400,
  "next_rss_at": "2026-10-27T09:10:00Z",
  "next_full_at": "2026-10-28T09:00:00Z",
  "last_rss_at": "2026-10-27T09:00:00Z",
  "last_full_at": null,
  "last_error": null,
  "dimensions": 6,
  "records": 291,
  "created_at": "2026-10-27T09:00:00Z"
}
```

**已知边界**：① RSS 仅 10 条窗口，两次轮询间变更超 10 条时旧的漏掉（全量兜底）；② 编辑旧标记是否 bump 到 feed 未验证，不依赖其做修改检测；③ 隐私条目在免登录 RSS 中不可见（配置 Cookie 的全量抓取可补全）；④ feed 结构无官方承诺，解析按容错设计。

### 4.11 封面代理（cache-then-serve）

**背景**：豆瓣图片 CDN（img1/2/3/9.doubanio.com）按子域名做 Referer 防盗链，策略不一致——浏览器直连时 img9 放行、img2/img3 一律 403（2026-10 实测），导致前端封面大量灰块（库内约 71% 记录在 img2/img3 上）。因此 `/marks` 响应中的 `cover` 一律改写为本端点的相对路径。

```
GET /api/v1/covers?u={urlencoded 豆瓣封面 URL}
```

语义：磁盘 `data/covers/{photo_id}` 命中直接返回文件；未命中带豆瓣 Referer 回源一次、落盘后返回（存量记录首次访问即自动"回填"）。响应带 `Cache-Control: public, max-age=2592000, immutable`（photo id 内容不可变）。

约束：**白名单代理**，仅接受 `https://*.doubanio.com` 且路径前缀为 `/view/photo/` 或 `/view/subject/` 的 URL（违反返回 400 `INVALID_ARGUMENT`），防止变成开放中继；走鉴权（`?token=` 兜底可用）但**豁免限流**（一页 20 张封面属正常连发）；回源并发受 `SYNC_COVER_CONCURRENCY`（默认 4）限制；回源失败返回 502 `COVER_UPSTREAM_FAILED`，不落负缓存（下次访问自动重试）。

**已知边界**：① 条目海报被豆瓣更换后本地旧图不更新（photo id 不变的场景无感知；换 id 则视为新图重新回源）；② 回源质量取决于服务器出口网络。

## 5. 数据模型

### Task

| 字段 | 类型 | 说明 |
|---|---|---|
| `task_id` | string | `sync_` + ULID，创建时间有序、URL 安全 |
| `user` | string | 归一化后的豆瓣 uid |
| `request` | object | 原始请求参数快照 |
| `status` | enum | 见状态机 |
| `progress[]` | array | 每 `(category,status)` 维度的 `{state, fetched, total}` |
| `items_count` | int | 已累计条数 |
| `error` | object? | 终态错误信息 |
| `created_at` / `started_at` / `finished_at` | timestamp | |
| `expires_at` | timestamp | 任务记录过期时间（默认 finished_at + 30d；过期仅清任务行，数据保留） |
| `source` | enum | `manual`（用户创建，默认）/ `auto`（订阅全量同步自动创建） |

### Dimension（调度与缓存元数据单元）

| 字段 | 类型 | 说明 |
|---|---|---|
| `uid` + `category` + `status` | string | 联合主键（缓存键） |
| `state` | enum | `idle` / `queued` / `running` / `ok` / `error` |
| `fetched_count` / `total` / `next_start` | int | 抓取进度与断点（数据本体在 records 表，本表只存调度状态） |
| `fetched_at` | timestamp | 最近成功抓取时间（TTL 判定依据） |
| `last_error` | string? | |

### Record（数据本体，`records` 表）

12 字段：`category, status, title, alt_title, my_rating, date, comment, subject_url, subject_id, cover, comment_id, intro`——一条标记一行，主键 `(uid, category, dim_status, subject_id)`，抓取时 `INSERT OR IGNORE` 天然去重；按维度长期保留，`refresh` 时整维度删除重写。RSS 增量同步共用同一路径。

### Subscription（订阅，`subscriptions` 表）

| 字段 | 类型 | 说明 |
|---|---|---|
| `uid` | string | 主键 |
| `enabled` | bool | 停用后不参与调度 |
| `rss_interval_secs` / `full_interval_secs` | int | 两个周期（下限 60s / 3600s） |
| `next_rss_ms` / `next_full_ms` | int | 下次执行时间（调度依据；rss 失败时兼作退避） |
| `last_rss_ms` / `last_full_ms` | timestamp? | 最近成功执行 |
| `last_guid` | string | feed 最新条目 guid（观察用游标；去重实际由 records 主键完成） |
| `rss_attempts` / `last_error` | int / string? | RSS 失败退避计数与最近错误 |

## 6. 任务状态机

```
                 ┌────────────┐
   POST /syncs │  queued    │────全部维度命中缓存────▶ completed（直接）
                 └─────┬──────┘
                       │ 取出调度（同 uid 串行）
                 ┌─────▼──────┐
        ┌────────│  running   │────────┐
        │        └─────┬──────┘        │
        │ 全部维度 ok   │ 任一维度失败    │ 全部维度失败
        │        ┌─────▼──────┐        │
        │        │ completed  │        │
        │        └────────────┘        │
        └───────▶ partial ◀────────────┘
                        │
                 ┌──────▼──────┐
                 │   failed    │（仅当无任何维度成功）
                 └─────────────┘
```

- `partial`：部分维度成功——`result` 仍可用（含成功维度汇总），成功维度数据已入库，`error.failed_dimensions` 列出失败维度
- 断点：`queued/running` 中断（进程重启、任务被删）后，Dimension 的 `checkpoint` 保留，下次同维度入队自动续爬
- 终态（`completed/partial/failed`）不可逆；重新抓取 = 新任务

## 7. 缓存与数据保留策略

| 项 | 默认值 | 说明 |
|---|---|---|
| 缓存 TTL | 24h | `fetched_at + TTL` 内 `fresh: true`，命中即不回源 |
| 强制刷新 | `refresh: true` | 跳过 TTL 判定；先清空该维度旧数据再重抓（期间该维度数据不可读直至抓取完成） |
| records 数据保留 | 永久 | 数据与任务生命周期解耦，仅 `refresh` 触发整维度重写；不随任务过期清理 |
| 任务记录保留 | 30 天 | `expires_at` 过期后后台清理任务行（数据不动） |
| 失败缓存 | 不缓存 | 维度失败只记 `last_error`，下次请求可重试 |
| 存储预估 | 单维度 ≤ 数千条 | 208 部电影 ≈ 226KB JSON；SQLite 完全够用 |

## 8. 限速与并发控制

四层防护，从外到内：

1. **客户端限流**：per-IP 令牌桶，默认 60 req/min（`429 TOO_MANY_REQUESTS`）
2. **任务并发上限**：全局运行中任务 ≤ 8，队列长度 ≤ 100（`429 TOO_MANY_TASKS`）
3. **豆瓣侧全局令牌桶**：所有维度共享，默认 ≥2.0s/请求 + 0–1s 抖动；同 uid 的维度严格串行，不同 uid 交错执行
4. **熔断器**：连续 5 次维度失败（豆瓣 403/超时）→ 全局暂停抓取 10 分钟，期间新任务入队但 `queued`，`server/info` 可见熔断状态

重试策略：403/418/5xx 指数退避（20s×n + 抖动，最多 5 次）；404 快速失败（维度标记 `error`，不重试）。

## 9. 错误响应规范

统一信封：

```json
{ "error": { "code": "TASK_NOT_FOUND", "message": "任务不存在或已过期", "task_id": "sync_..." } }
```

| HTTP | code | 场景 |
|---|---|---|
| 400 | `INVALID_ARGUMENT` | uid 非法 / 枚举值错误 / 分页参数越界 |
| 401 | `UNAUTHORIZED` | 启用鉴权且 token 缺失/错误 |
| 404 | `TASK_NOT_FOUND` / `DIMENSION_NOT_CACHED` | 资源不存在 |
| 409 | `TASK_NOT_FINISHED` | 任务未终态就取 result |
| 429 | `TOO_MANY_REQUESTS` / `TOO_MANY_TASKS` | 客户端限流 / 队列满 |
| 500 | `INTERNAL` | 未预期错误 |
| 503 | `CIRCUIT_OPEN` | 熔断期间拒绝新任务（可选行为：也可只入队不报错） |

任务内部的豆瓣侧失败不映射为 API 错误码——体现在任务终态（`partial/failed`）与 `error.code`（`DOUBAN_BLOCKED` / `DOUBAN_TIMEOUT` / `PAGE_NOT_FOUND` / `PARSE_ERROR`）。

## 10. 认证与安全

- **默认无鉴权**（本地/内网部署友好）；设环境变量 `SYNC_SERVER_TOKEN` 后启用 `Authorization: Bearer <token>`（`/healthz` 除外）
- **输入收敛**：`user` 归一化为 uid 白名单字符集（`[A-Za-z0-9._-]+`），杜绝 SSRF/路径注入
- **无敏感凭据**：服务本身免登录，不存储任何豆瓣 Cookie；提醒部署者勿把个人 Cookie 混入此服务
- **CORS**：默认关闭；`SYNC_SERVER_CORS_ORIGINS` 配置允许来源
- **公开仓库提醒**：本服务只暴露公开数据，但若部署到公网，建议加鉴权 + 限流，避免被当作免费豆瓣抓取代理滥用

## 11. 其他横切关注点

- **可观测性**：结构化日志（task_id / dimension 贯穿）；`server/info` 暴露队列深度；未来可加 `/metrics`（Prometheus）
- **优雅停机**：SIGTERM 后停止取新任务，运行中维度保存 checkpoint 后退出，重启续跑
- **版本策略**：路径前缀 `/api/v1`；不兼容变更升 `/api/v2`
- **配置**：全部经环境变量（端口、token、TTL、限速参数、存储路径），无配置文件

## 12. 非目标（v1 明确不做）

- ❌ 写操作（标记、评论豆瓣）——免登录路径根本不支持
- ❌ 登录态接口（个人私密数据）——引入 Cookie 就是另一个信任模型与风险等级
- ❌ 条目详情/搜索 API——只做"用户标记"这一件事
- ❌ 用户注册/多租户——单实例工具型服务
- ❌ 实时推送（WebSocket）——SSE 已够，v1 甚至可只做轮询

## 13. 开放问题（实现前需拍板）

1. ~~**SSE 是否进 v1？**~~ 已决：SSE 已实现（broadcast 通知 + 快照 + `?token=` 兜底鉴权），轮询与推流并存
2. **缓存 TTL 24h 是否合适？** 重度用户标记频率高于此；可考虑"TTL + 手动 refresh"双轨（当前设计已支持）
3. **`/users/{uid}/marks` 是否允许自动触发抓取**（`?fetch=async`）？当前设计为"只读缓存 + 404 引导"，避免该接口变成免费抓取入口（建议维持）
4. **公网部署时的鉴权默认值**：是否改为"默认必须配置 token 才能启动"？（建议：默认不强制，README 明示公网风险）
5. ~~**断点续爬的 checkpoint 存放位置**~~ 已决：并入 Dimension 表（next_start 列）

---

*附：实现——axum + tokio 异步框架，sqlx/SQLite 存任务与数据，tower 中间件做限流；抓取解析逻辑在库模块 `crawler`，服务器为唯一二进制形态。*
