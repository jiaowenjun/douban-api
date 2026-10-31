# 豆瓣数据同步服务器 · 管理后台

基于 [Ant Design Pro Components](https://procomponents.ant.design/)（ProLayout / ProTable / ModalForm）+ Vite + React + pnpm 的服务器管理页面，对接 `weimo-douban-server` 的 `/api/v1` 接口。

## 功能（刻意保持精简）

| 页面 | 功能 |
|---|---|
| **概览** `/` | 服务状态、抓取队列、熔断器、鉴权状态、运行参数、全站最近任务（跨用户，含订阅触发）、订阅同步管理（对任意用户发起/取消订阅）；每 30s 自动刷新 |
| **用户** `/users` | 全用户列表（已入库资料 ∪ 已有数据 ∪ 已订阅 ∪ 建过任务，按记录数降序）：记录/维度计数、订阅状态、最近任务与抓取时间；行内直达数据浏览、任务筛选、创建同步、订阅开关 |
| **同步任务** `/tasks` | 全站任务列表（跨所有用户，默认含订阅触发的 auto 任务；可按用户/状态/来源筛选、游标翻页）、新建同步任务（表单内选用户或输入新 uid 冷启动）、详情抽屉（SSE 进度推流 + 轮询降级、错误信息、维度汇总与数据浏览跳转）、删除任务；有活跃任务时列表自动轮询；支持 `?user=` 深链 |
| **数据浏览** `/marks` | 读取服务器 SQLite 内的书影音标记（不触发抓取）：封面、条目链接、星级评分、日期、短评，服务端分页；页内切换用户（默认选中记录最多的用户）；支持 `?uid=&category=&status=` 深链（任务详情「浏览数据」跳转落地页） |

管理台为**全后端视角**：无需设置用户身份。右上角「连接」只配 API 地址与 Bearer Token（均存 localStorage）：

- 默认 `/api/v1`（同源部署或开发期 Vite 代理，无需 CORS）
- 跨源部署填完整地址（如 `http://192.168.1.10:8080/api/v1`），需服务器设置 `SYNC_SERVER_CORS_ORIGINS`
- 服务器设置了 `SYNC_SERVER_TOKEN` 时在此填写

## 开发

```bash
# 1. 启动后端（backend/ 目录，默认 127.0.0.1:8080）
cd backend && cargo run --bin weimo-douban-server

# 2. 启动前端（另一个终端，Vite 已配置 /api 与 /healthz 代理到 8080）
cd frontend/admin && pnpm install && pnpm dev
# 打开 http://localhost:5173
```

## 构建与部署

```bash
pnpm build   # tsc 类型检查 + vite 打包到 dist/
```

`dist/` 是纯静态文件，部署方式任选：

- **同源部署（推荐，免 CORS）**：用 nginx / Caddy 等静态服务器托管 `dist/`，并把 `/api` 与 `/healthz` 反代到 `weimo-douban-server`；SPA 需配置历史回退（try_files → index.html）。
- **跨源部署**：任意静态托管 + 右上角「连接」里填 API 完整地址 + 服务器开 `SYNC_SERVER_CORS_ORIGINS`。

## 代码结构

```
src/
  api.ts                    # fetch 封装：base/token、错误信封解析、SSE 地址
  types.ts                  # 与 backend/src/server/service.rs 响应结构一一对应
  labels.tsx                # 状态/维度中文标签、Tag 渲染
  App.tsx                   # ProLayout 布局 + 路由 + 连接设置
  user-profile.tsx          # useUserProfile / UserAvatar / UserCell（用户列通用渲染）
  pages/
    Overview.tsx            # 概览（服务状态 + 订阅管理 + 全站最近任务）
    Users.tsx               # 全用户管理（列表 + 行内同步/订阅/跳转）
    Tasks.tsx               # 抓取任务（ProTable + 游标翻页 + 活跃轮询 + 用户/来源筛选）
    Marks.tsx               # 库内数据浏览（页内切换用户，支持 URL 参数预填）
  components/
    CreateTaskModal.tsx     # 新建抓取（ModalForm，表单内选用户/输入新 uid）
    TaskDetailDrawer.tsx    # 任务详情抽屉（SSE 进度 + 维度汇总 + 数据浏览跳转）
```

## 已知取舍

- 任务列表接口只返回摘要字段（无进度），进度仅在详情抽屉中轮询展示，避免 N+1 请求
- 进度百分比由服务端计算（`percent` 字段），前端不做二次计算
- 未做登录页：服务器鉴权为可选 Bearer Token，用右上角「连接」配置即可
