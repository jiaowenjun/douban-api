# 豆瓣导出服务器 · 管理后台

基于 [Ant Design Pro Components](https://procomponents.ant.design/)（ProLayout / ProTable / ModalForm）+ Vite + React + pnpm 的服务器管理页面，对接 `douban-export-server` 的 `/api/v1` 接口。

## 功能（刻意保持精简）

| 页面 | 功能 |
|---|---|
| **概览** `/` | 服务状态、抓取队列、熔断器、鉴权状态、运行参数、最近任务；每 5s 自动刷新 |
| **导出任务** `/tasks` | 任务列表（按用户/状态筛选、游标翻页）、新建抓取任务、详情抽屉（SSE 进度推流 + 轮询降级、错误信息、维度汇总与数据浏览跳转）、删除任务；有活跃任务时列表自动轮询 |
| **数据浏览** `/marks` | 读取服务器 SQLite 内的书影音标记（不触发抓取）：封面、条目链接、星级评分、日期、短评，服务端分页；支持 `?uid=&category=&status=` 预填（任务详情「浏览数据」跳转落地页） |

右上角「连接」可修改 API 地址与 Bearer Token（存 localStorage）：

- 默认 `/api/v1`（同源部署或开发期 Vite 代理，无需 CORS）
- 跨源部署填完整地址（如 `http://192.168.1.10:8080/api/v1`），需服务器设置 `EXPORT_SERVER_CORS_ORIGINS`
- 服务器设置了 `EXPORT_SERVER_TOKEN` 时在此填写

## 开发

```bash
# 1. 启动后端（仓库根目录，默认 127.0.0.1:8080）
cargo run --bin douban-export-server

# 2. 启动前端（另一个终端，Vite 已配置 /api 与 /healthz 代理到 8080）
cd web && pnpm install && pnpm dev
# 打开 http://localhost:5173
```

## 构建与部署

```bash
pnpm build   # tsc 类型检查 + vite 打包到 dist/
```

`dist/` 是纯静态文件，部署方式任选：

- **同源部署（推荐，免 CORS）**：用 nginx / Caddy 等静态服务器托管 `dist/`，并把 `/api` 与 `/healthz` 反代到 `douban-export-server`；SPA 需配置历史回退（try_files → index.html）。
- **跨源部署**：任意静态托管 + 右上角「连接」里填 API 完整地址 + 服务器开 `EXPORT_SERVER_CORS_ORIGINS`。

## 代码结构

```
src/
  api.ts                    # fetch 封装：base/token、错误信封解析、SSE 地址
  types.ts                  # 与 src/server/service.rs 响应结构一一对应
  labels.tsx                # 状态/维度中文标签、Tag 渲染
  App.tsx                   # ProLayout 布局 + 路由 + 连接设置
  pages/
    Overview.tsx            # 概览
    Tasks.tsx               # 抓取任务（ProTable + 游标翻页 + 活跃轮询）
    Marks.tsx               # 库内数据浏览（支持 URL 参数预填）
  components/
    CreateTaskModal.tsx     # 新建抓取（ModalForm）
    TaskDetailDrawer.tsx    # 任务详情抽屉（SSE 进度 + 维度汇总 + 数据浏览跳转）
```

## 已知取舍

- 任务列表接口只返回摘要字段（无进度），进度仅在详情抽屉中轮询展示，避免 N+1 请求
- 进度百分比由服务端计算（`percent` 字段），前端不做二次计算
- 未做登录页：服务器鉴权为可选 Bearer Token，用右上角「连接」配置即可
