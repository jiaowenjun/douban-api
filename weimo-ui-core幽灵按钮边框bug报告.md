# weimo-ui-core 幽灵图标按钮透出 UA 默认边框（bug 报告 / 上游修复建议）

- **组件**：`GhostIconButton`（`icon-button--ghost` 变体）
- **包**：`weimo-ui-core`，已验证 **0.2.1、0.2.2** 受影响（凡消费方无表单 reset，所有带 ghost 变体的版本均应受影响）
- **消费方环境**：weimo-douban `frontend/web`（weimo-ui-core 0.2.2 + weimo-ui-template 0.4.3，**未引入 Tailwind**，index.css 仅 `@import 'weimo-ui-core/styles/tokens.css'` 与 `@import 'weimo-ui-template/styles/base.css'`）
- **对照环境**：weimo-biji `frontend/web`（同系列组件 + `@import "tailwindcss"`），表现正常
- **关联前案**：b308eeb「fix: frosted 图标按钮补透明底,磨砂材质不再依赖消费方 reset」（0.2.2 发布）。本案是同一族问题的**边框半边**，建议一并根治。

## 一、问题表现

无 Tailwind / 无表单 reset 的消费站点里，`GhostIconButton` 周围出现一圈细描边（深色主题下呈灰色圆环），预期应为完全无边框。

实测计算样式（macOS WebKit，深色主题，:5177 页面上直接读取）：

| 属性 | ghost 按钮（侧边栏登录/设置） | 预期 |
|---|---|---|
| `border-width` | `2px` | `0` / `none` |
| `border-style` | `outset` | `none` |
| `border-color` | `rgb(255, 255, 255)` | — |
| `appearance` | `auto` | — |
| `outline` | `0 none`（排除焦点环） | — |

`border-style: outset` 是 WebKit 原生 `<button>` 边框的典型特征——这圈边**不是组件画的，而是 UA 默认样式透出来的**。各引擎具体值不同（Chromium 为 `ButtonBorder` 灰），但结论一致：只要没有样式把 border 归零，UA 边框就会显示。

排除项：

- 不是焦点环：同一界面**两个**按钮同时有圈（焦点只会在一个元素上），且 `outline` 为 `0`。
- 不是 hover/popup 残留：`matches(':hover')`、`data-popup-open` 均为假。

## 二、根因

三层因素叠加：

1. **UA 层**：`<button>` 自带默认边框（WebKit `2px outset`）。
2. **组件层**：`src/components/controls/icon-button/icon-button.css` 中 ghost 变体只 reset 了背景，**没有任何 border 声明**（0.2.2 源码 97–99 行）：

   ```css
   .icon-button--ghost {
     background: transparent;
   }
   ```

   对比 frosted 变体：它叠加的 `.frosted-surface` 自带 `border: 1px solid transparent`（`src/components/surfaces/frosted-surface/frosted-surface.css`），自带边框几何，所以 **frosted 系按钮在同样的消费站点上从不露圈**——本案只影响 ghost 变体。

3. **站点层**：`weimo-ui-template` 的 `base.css`（`@layer base`）只对 button 做 `font: inherit` 等归一化，**不 reset border/background**；而官方两个示例站 `site/src/styles/global.css` 首行都是 `@import "tailwindcss"`，v4 preflight 把所有元素 border 归零，**恰好把本 bug 永久遮蔽**——这就是它从未被发现的原因（与上一轮 frosted UA 灰盘问题的遮蔽机制完全相同）。

结论：组件库对「消费方一定有表单 reset」存在隐式依赖，但包本身不声明 tailwind 依赖，README 示例之外「无 Tailwind 消费」是合法形态。ghost 的背景已经自带 reset（`background: transparent`），唯独漏了 border。

## 三、复现方式

**方式 A：现成站点（最快）**

weimo-douban `frontend/web`（本仓库）：`pnpm exec vite --port 5177 --strictPort` 起服务，打开 `http://localhost:5177/#/movie`，侧边栏顶部栏右侧「登录」「设置」两个幽灵按钮各有一圈描边；DevTools 查看任一按钮计算样式为 `border: 2px outset rgb(255,255,255)`（WebKit）。

**方式 B：最小复现**

```bash
pnpm create vite ghost-repro --template react-ts && cd ghost-repro
pnpm add weimo-ui-core
```

```tsx
// main.tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { GhostIconButton } from 'weimo-ui-core/components/ghost-icon-button'
import { LogIn } from 'lucide-react'
import './index.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <GhostIconButton aria-label="登录" size="sm">
      <LogIn aria-hidden="true" />
    </GhostIconButton>
  </StrictMode>,
)
```

```css
/* index.css —— 关键：不引 tailwind / 任何 reset，只引 tokens */
@import 'weimo-ui-core/styles/tokens.css';
```

打开页面即可见按钮带 UA 边框；在 index.css 追加 `button { border: none }` 后消失，证明边框来自 UA 默认且组件未 reset。

**反向验证（遮蔽效应）**：临时注释掉官方 site 的 `global.css` 首行 `@import "tailwindcss"`，site 里的 ghost 按钮即出现同样的圈——可直接在官方站复现。

## 四、上游解决方案

**首选：ghost 变体自带 border reset**（与背景 reset 的既有哲学对齐——「不依赖消费方 reset」，见 b308eeb）：

```diff
   .icon-button--ghost {
     background: transparent;
+
+    /* 幽灵按钮无边框语义；UA 默认 button 边框（如 WebKit 2px outset）
+       会在无表单 reset 的消费站点透出，故不依赖消费方 reset（与背景对齐）。 */
+    border: none;
   }
```

为什么只改 ghost、不动 `.icon-button` 基类或 `frosted-surface`：

- frosted 依赖 `border: 1px solid transparent` 撑几何（`bordered` 变体还依赖可见 border-color 过渡）。若在基类加 `border: none`，基类与 `.frosted-surface` 同层同特异性（0,1,0），结果取决于 CSS 打包顺序，存在回归风险；收窄到 ghost 变体则零风险。
- ghost 无任何依赖 border 几何的状态（hover/active 均为背景变化，border-color 过渡对 `none` 无副作用）。

**可选加固（防同类问题）**：`weimo-ui-template` 的 `src/styles/base.css` 在 `@layer base` 中补 preflight 等价的表单控件 border reset，使 template 的消费站点整体不再依赖 Tailwind。注意必须放 `@layer base`（低于 `@layer components`），否则会压过 frosted 的自有描边。

**回归测试建议**：现有 site 契约/截图测试全部跑在带 Tailwind 的站点上，永远测不出这类问题。建议加一个**无 preflight 的 fixture**（site 内一条不引 tailwindcss 的路由，或独立最小页面），断言 ghost 按钮计算样式 `border-style: none`；也可对上一轮的背景问题补同类断言（frosted `background-color: transparent`）。

## 五、消费方临时 workaround（上游发版后可删）

weimo-douban `frontend/web/src/index.css` 追加：

```css
@layer base {
  button {
    border: none;
  }
}
```

必须放 `@layer base`：非分层样式优先级高于库的 `@layer components`，裸写会把 frosted `bordered` 变体（如顶栏主题按钮）刻意保留的 1px 描边一并干掉。

---

*实测证据：macOS WebKit（IAB）`getComputedStyle` 读数；两份对照截图（weimo-douban 带圈 / weimo-biji 无圈）见报告附图。*
