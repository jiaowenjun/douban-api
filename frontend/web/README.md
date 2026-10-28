# weimo-douban web

单用户豆瓣标记展示站点（公开侧前端）。站点外壳来自
[weimo-ui-template](https://www.npmjs.com/package/weimo-ui-template) 的
`SiteTemplate`：侧栏导航（699px 断点自动折叠为抽屉）、顶栏、亮/暗/跟随系统
三态主题切换、文档标题管理。

## 技术栈

- Vite 8 + React 19（React Compiler）+ TypeScript
- [weimo-ui-template](https://www.npmjs.com/package/weimo-ui-template) —
  SiteTemplate 站点外壳（peer 依赖 `weimo-ui-core`、`@base-ui/react`）
- react-router 8（HashRouter，静态托管免服务端路由配置）

## 结构

```
src/
├── App.tsx               # SiteTemplate 外壳 + 路由
├── index.css             # tokens.css + base.css（组件自身 CSS 随 JS 自动引入）
└── site/
    ├── navigation.ts     # 导航分组（电影/图书/音乐 × 看过在看想看…）与路由参数解析
    └── pages/
        └── marks-page.tsx    # 标记列表页占位，待接入 GET /api/v1/users/{uid}/marks
```

路由为 `/:category/:status`（category: movie|book|music，status:
collect|do|wish），非法组合重定向到 `/movie/collect`。

## 开发

```sh
pnpm dev      # Vite 开发服务器（注意本机只监听 IPv6 localhost）
pnpm build    # tsc -b + vite build
pnpm lint     # oxlint
```
