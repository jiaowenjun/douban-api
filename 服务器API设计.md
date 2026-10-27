# 豆瓣导出服务器 API 设计（v1）

> 状态：**已实现**（v0.2.0，SSE 按 §13 建议 v1 未做轮询）
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

把现有 CLI（`douban-export`）的能力服务化：**免登录导出任意豆瓣用户的公开书影音标记与短评**。

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
                  
             存储层：SQLite（任务+缓存数据） + 文件（导出产物）
```

组件职责：

- **API 层**：参数校验、鉴权（可选）、客户端限流、路由
- **任务管理器**：任务队列与状态机；对同一 `uid+category+status` 维度做 single-flight（去重合并）
- **抓取器**：移植现有 CLI 的抓取/解析/重试/断点逻辑（含 Referer、两代页面结构兼容）
- **存储**：SQLite 存任务记录与缓存数据（Record 结构 12 字段不变）；导出文件（csv/json/md/xlsx）落盘

## 2. 核心设计决策与理由

### D1：异步任务模型（而非同步等待）

全量抓取 ≥1 分钟，超出任何合理的 HTTP 超时。`POST /exports` 创建任务后立即返回 `202`，客户端轮询（或 SSE）拿进度，完成后下载。

对极小数据量提供**有限同步等待**逃生门：`?wait_timeout=N`（N ≤ 30s），超时未完成仍返回任务 ID，不阻塞。

### D2：按维度缓存，任务结果可直接命中

缓存键 = `(uid, category, status)`，TTL 默认 24h。同参数请求在缓存有效期内**秒回**（不碰豆瓣）。`refresh: true` 强制重抓。

理由：书影音标记变化频率低（天级），而每次回源成本高（多页 × 限速）且消耗服务器 IP 信誉。

### D3：single-flight——同维度并发请求合并

同一 `(uid, category, status)` 已有运行中任务时，`POST /exports` 不新建任务，直接返回已有 task（幂等）。防止多个客户端同时触发对同一用户的重复抓取。

### D4：豆瓣侧全局节流是服务生存的前提

所有抓取经过**单一全局令牌桶**（默认 ≥2s/请求）+ 同 uid 串行 + 连续失败熔断（详见第 8 节）。对外服务再大的并发，到豆瓣侧也恒定为低速串行流。这也是把"服务器"与"CLI 直接抓"区分开的核心价值：共享缓存、保护出口 IP。

## 3. API 一览

Base URL：`http://{host}:8080/api/v1`（默认端口 8080，可配置）

| # | 方法 | 路径 | 用途 |
|---|---|---|---|
| 1 | POST | `/exports` | 创建导出任务（核心入口） |
| 2 | GET | `/exports/{task_id}` | 查询任务状态与进度 |
| 3 | GET | `/exports` | 任务列表（过滤、游标分页） |
| 4 | DELETE | `/exports/{task_id}` | 删除任务及其产物 |
| 5 | GET | `/exports/{task_id}/result` | 结果元数据（文件清单 + 汇总） |
| 6 | GET | `/exports/{task_id}/result/{category}_{status}.{ext}` | 下载单个导出文件 |
| 7 | GET | `/users/{uid}/marks` | 直接读缓存数据（JSON，分页） |
| 8 | GET | `/exports/{task_id}/events` | 任务进度 SSE 推流（可选实现） |
| 9 | GET | `/server/info` | 服务元信息与限速/缓存配置 |
| 10 | GET | `/healthz` | 健康检查（在 `/api/v1` 之外） |

## 4. 接口详细设计

### 4.1 创建导出任务

```
POST /api/v1/exports
Content-Type: application/json
```

请求体：

```json
{
  "user": "wenjun30",
  "categories": ["movie", "book", "music"],
  "statuses": ["collect"],
  "formats": ["csv", "json"],
  "refresh": false,
  "wait_timeout": 0
}
```

| 字段 | 类型 | 默认 | 校验 |
|---|---|---|---|
| `user` | string | 必填 | 豆瓣用户 ID 或 `/people/{id}/` URL；复用 CLI 的 ID 规则（`[A-Za-z0-9._-]+`） |
| `categories` | string[] | `["movie","book","music"]` | 枚举 |
| `statuses` | string[] | `["collect"]` | `collect` / `do` / `wish` |
| `formats` | string[] | `["csv","json"]` | `csv` / `json` / `md` / `xlsx` |
| `refresh` | bool | `false` | `true` 时忽略未过期缓存，强制回源 |
| `wait_timeout` | int(秒) | `0` | `0`–`30`；>0 时服务端最多等待该时长，完成即返回 `completed` |

响应 `202 Accepted`：

```json
{
  "task_id": "exp_01JD6Z8W4R9TQK5M2VX3YB7C8D",
  "user": "wenjun30",
  "status": "queued",
  "cached_dimensions": 1,
  "created_dimensions": 2,
  "created_at": "2026-10-27T10:00:00+08:00",
  "url": "/api/v1/exports/exp_01JD6Z8W4R9TQK5M2VX3YB7C8D"
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
GET /api/v1/exports/{task_id}
```

响应 `200`：

```json
{
  "task_id": "exp_01JD6Z8W4R9TQK5M2VX3YB7C8D",
  "user": "wenjun30",
  "status": "running",
  "request": { "categories": ["movie","book","music"], "statuses": ["collect"],
               "formats": ["csv","json"], "refresh": false },
  "progress": [
    { "category": "movie", "status": "collect", "state": "running",
      "fetched": 45, "total": 209, "percent": 21 },
    { "category": "book",  "status": "collect", "state": "done",
      "fetched": 55, "total": 55, "percent": 100 },
    { "category": "music", "status": "collect", "state": "queued",
      "fetched": 0,  "total": null, "percent": null }
  ],
  "items_count": 100,
  "error": null,
  "percent": 38,
  "created_at": "2026-10-27T10:00:00+08:00",
  "finished_at": null,
  "result_url": "/api/v1/exports/exp_01JD6Z8W4R9TQK5M2VX3YB7C8D/result",
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
GET /api/v1/exports?user=wenjun30&status=running&limit=20&cursor=exp_01JD...
```

| 参数 | 说明 |
|---|---|
| `user` | 按用户过滤 |
| `status` | `queued` / `running` / `completed` / `partial` / `failed` |
| `limit` | 默认 20，上限 100 |
| `cursor` | 游标（上一页最后一条 task_id），按创建时间倒序 |

响应 `200`：

```json
{
  "tasks": [ { ...同 4.2 摘要字段... } ],
  "next_cursor": "exp_01JD6A...",
  "has_more": true
}
```

### 4.4 删除任务

```
DELETE /api/v1/exports/{task_id}
```

- `204 No Content`：删除任务记录与导出文件
- **不删除缓存数据**（缓存属于维度级共享资源，见 D2/D3）
- `404 TASK_NOT_FOUND`
- 运行中任务允许删除：先取消再清理（`state: running` 的维度中断，断点保留在其缓存记录中）

### 4.5 结果元数据

```
GET /api/v1/exports/{task_id}/result
```

响应 `200`：

```json
{
  "task_id": "exp_01JD6Z8W4R9TQK5M2VX3YB7C8D",
  "user": "wenjun30",
  "summary": {
    "total": 291,
    "with_comment": 272,
    "by_category_status": { "movie·看过": 208, "book·读过": 55, "music·听过": 28 }
  },
  "files": [
    { "name": "wenjun30_movie_collect.csv",  "category": "movie", "status": "collect",
      "format": "csv",  "size": 179479, "sha256": "9f2a...", "items": 208,
      "url": "/api/v1/exports/exp_.../result/wenjun30_movie_collect.csv" },
    { "name": "wenjun30_movie_collect.json", "format": "json", "size": 226152,
      "sha256": "c81b...", "items": 208, "url": "..." }
  ],
  "expires_at": "2026-11-03T10:00:00+08:00"
}
```

`status` 非终态时返回 `409 TASK_NOT_FINISHED`（附当前 status 与 progress 摘要）。

### 4.6 下载导出文件

```
GET /api/v1/exports/{task_id}/result/{file_name}
```

- 文件名即 4.5 返回的 `name`（如 `wenjun30_movie_collect.csv`），与 CLI 输出命名**保持一致**
- Content-Type：csv → `text/csv; charset=utf-8`（带 BOM）；json → `application/json; charset=utf-8`；md → `text/markdown; charset=utf-8`；xlsx → `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet`
- 带 `ETag`（sha256 前 16 位）与 `Last-Modified`，支持 `If-None-Match` → `304`
- `Content-Disposition: attachment; filename="..."`

错误：`404 TASK_NOT_FOUND` / `404 FILE_NOT_FOUND`。

### 4.7 直接读缓存数据

```
GET /api/v1/users/{uid}/marks?category=movie&status=collect&page=1&page_size=50
```

定位：**只读缓存**，给应用（而非浏览器下载）用的结构化数据接口。不触发抓取。

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
  "message": "该维度无缓存，请先创建导出任务",
  "hint": "POST /api/v1/exports {\"user\":\"wenjun30\",\"categories\":[\"movie\"],\"statuses\":[\"collect\"]}" } }
```

### 4.8 任务进度 SSE（可选实现）

```
GET /api/v1/exports/{task_id}/events
Accept: text/event-stream
```

事件格式（与 4.2 的 progress 同构，轮询的替代方案）：

```
event: progress
data: {"task_id":"exp_...","status":"running","progress":[...],"items_count":100}

event: done
data: {"task_id":"exp_...","status":"completed","result_url":"..."}

event: error
data: {"task_id":"exp_...","status":"failed","error":{...}}
```

任务进入终态后发送 `done`/`error` 并关闭流。

### 4.9 服务元信息

```
GET /api/v1/server/info
```

```json
{
  "name": "douban-export-server",
  "version": "0.1.0",
  "crawl": { "min_interval_seconds": 2.0, "max_concurrent_users": 2,
             "circuit_breaker": { "threshold": 5, "cooldown_seconds": 600 } },
  "cache": { "ttl_seconds": 86400, "retention_task_days": 30, "retention_file_days": 7 },
  "queue": { "running": 1, "queued": 0 }
}
```

### 4.10 健康检查

```
GET /healthz    → 200 {"status":"ok"}
```

不在 `/api/v1` 前缀下、不鉴权，供容器探活。

## 5. 数据模型

### Task

| 字段 | 类型 | 说明 |
|---|---|---|
| `task_id` | string | `exp_` + ULID，创建时间有序、URL 安全 |
| `user` | string | 归一化后的豆瓣 uid |
| `request` | object | 原始请求参数快照 |
| `status` | enum | 见状态机 |
| `progress[]` | array | 每 `(category,status)` 维度的 `{state, fetched, total}` |
| `items_count` | int | 已累计条数 |
| `error` | object? | 终态错误信息 |
| `created_at` / `started_at` / `finished_at` | timestamp | |
| `expires_at` | timestamp | 文件过期时间（默认 finished_at + 7d） |

### Dimension（缓存与调度单元）

| 字段 | 类型 | 说明 |
|---|---|---|
| `uid` + `category` + `status` | string | 联合主键（缓存键） |
| `state` | enum | `idle` / `queued` / `running` / `ok` / `error` |
| `records` | blob | Record 数组（SQLite JSON 列） |
| `fetched_at` | timestamp | 最近成功抓取时间（TTL 判定依据） |
| `checkpoint` | json | 断点（next_start / 页进度），复用 CLI 的 ckpt 结构 |
| `last_error` | string? | |

### Record（不变）

沿用 CLI 的 12 字段：`category, status, title, alt_title, my_rating, date, comment, subject_url, subject_id, cover, comment_id, intro`——服务器只做搬运，不改字段语义，保证 CLI 导出文件与 API 数据可直接互证。

## 6. 任务状态机

```
                 ┌────────────┐
   POST /exports │  queued    │────全部维度命中缓存────▶ completed（直接）
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

- `partial`：部分维度成功——`result` 仍可用（含成功维度的文件），`error.failed_dimensions` 列出失败维度
- 断点：`queued/running` 中断（进程重启、任务被删）后，Dimension 的 `checkpoint` 保留，下次同维度入队自动续爬
- 终态（`completed/partial/failed`）不可逆；重新抓取 = 新任务

## 7. 缓存与数据保留策略

| 项 | 默认值 | 说明 |
|---|---|---|
| 缓存 TTL | 24h | `fetched_at + TTL` 内 `fresh: true`，命中即不回源 |
| 强制刷新 | `refresh: true` | 跳过 TTL 判定；旧数据在抓取成功前保持可读（不删旧写新失败态） |
| 结果文件保留 | 7 天 | `expires_at` 过期后后台清理；任务记录保留 30 天 |
| 失败缓存 | 不缓存 | 维度失败只记 `last_error`，下次请求可重试 |
| 存储预估 | 单维度 ≤ 数千条 | 208 部电影 JSON ≈ 226KB；SQLite 完全够用 |

## 8. 限速与并发控制

四层防护，从外到内：

1. **客户端限流**：per-IP 令牌桶，默认 60 req/min（`429 TOO_MANY_REQUESTS`）
2. **任务并发上限**：全局运行中任务 ≤ 8，队列长度 ≤ 100（`429 TOO_MANY_TASKS`）
3. **豆瓣侧全局令牌桶**：所有维度共享，默认 ≥2.0s/请求 + 0–1s 抖动（与 CLI 一致）；同 uid 的维度严格串行，不同 uid 交错执行
4. **熔断器**：连续 5 次维度失败（豆瓣 403/超时）→ 全局暂停抓取 10 分钟，期间新任务入队但 `queued`，`server/info` 可见熔断状态

重试策略沿用 CLI：403/418/5xx 指数退避（20s×n + 抖动，最多 5 次）；404 快速失败（维度标记 `error`，不重试）。

## 9. 错误响应规范

统一信封：

```json
{ "error": { "code": "TASK_NOT_FOUND", "message": "任务不存在或已过期", "task_id": "exp_..." } }
```

| HTTP | code | 场景 |
|---|---|---|
| 400 | `INVALID_ARGUMENT` | uid 非法 / 枚举值错误 / 分页参数越界 |
| 401 | `UNAUTHORIZED` | 启用鉴权且 token 缺失/错误 |
| 404 | `TASK_NOT_FOUND` / `FILE_NOT_FOUND` / `DIMENSION_NOT_CACHED` | 资源不存在 |
| 409 | `TASK_NOT_FINISHED` | 任务未终态就取 result |
| 429 | `TOO_MANY_REQUESTS` / `TOO_MANY_TASKS` | 客户端限流 / 队列满 |
| 500 | `INTERNAL` | 未预期错误 |
| 503 | `CIRCUIT_OPEN` | 熔断期间拒绝新任务（可选行为：也可只入队不报错） |

任务内部的豆瓣侧失败不映射为 API 错误码——体现在任务终态（`partial/failed`）与 `error.code`（`DOUBAN_BLOCKED` / `DOUBAN_TIMEOUT` / `PAGE_NOT_FOUND` / `PARSE_ERROR`）。

## 10. 认证与安全

- **默认无鉴权**（本地/内网部署友好）；设环境变量 `EXPORT_SERVER_TOKEN` 后启用 `Authorization: Bearer <token>`（`/healthz` 除外）
- **输入收敛**：`user` 归一化为 uid 白名单字符集（`[A-Za-z0-9._-]+`），杜绝 SSRF/路径注入；`file_name` 仅允许服务端生成的白名单集合
- **无敏感凭据**：服务本身免登录，不存储任何豆瓣 Cookie；提醒部署者勿把个人 Cookie 混入此服务
- **CORS**：默认关闭；`EXPORT_SERVER_CORS_ORIGINS` 配置允许来源
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

1. **SSE 是否进 v1？** 轮询已满足需求，SSE 可砍掉减复杂度（建议：v1 只轮询，`events` 接口留占位）
2. **缓存 TTL 24h 是否合适？** 重度用户标记频率高于此；可考虑"TTL + 手动 refresh"双轨（当前设计已支持）
3. **`/users/{uid}/marks` 是否允许自动触发抓取**（`?fetch=async`）？当前设计为"只读缓存 + 404 引导"，避免该接口变成免费抓取入口（建议维持）
4. **公网部署时的鉴权默认值**：是否改为"默认必须配置 token 才能启动"？（建议：默认不强制，README 明示公网风险）
5. **断点续爬的 checkpoint 存放位置**：并入 Dimension 表（当前设计）vs 独立文件（CLI 兼容）？（建议：并入表，CLI 的 ckpt 文件仅本地用）

---

*附：实现建议（非约束）——axum + tokio 做异步框架，SQLite（rusqlite/sqlx）存任务与缓存，tower 中间件做限流；抓取器把现有 `parse_items`/`fetch_html` 逻辑原样移植为库模块，CLI 与 server 共用同一 crate 内的 lib。*
