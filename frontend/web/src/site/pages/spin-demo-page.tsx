import { RefreshCw } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { RefObject } from 'react'
import { useNavigate } from 'react-router'
import { LabeledSwitch } from 'weimo-ui-core/components/labeled-switch'
import { TextButton } from 'weimo-ui-core/components/text-button'
import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { MiniSiteTemplate } from 'weimo-ui-template/components/mini-site-template'

import type { DoubanDims } from '../api'
import { FacetTree } from '../facet-tree'
import './spin-demo-page.css'

/** 是否运行在 Tauri 桌面 App 壳内（口径同 welcome-page 的 IN_TAURI_APP：
 *  Tauri 2 在壳内 webview 文档起始注入 window.isTauri，普通浏览器永不
 *  定义）；诊断面板按它标注运行环境 */
const IN_TAURI_APP =
  typeof window !== 'undefined' &&
  (window as { isTauri?: boolean }).isTauri === true

/** 演示数据：构造 6 条影视条目——is_tv 派生三形态各就各位（false=电影
 *  2、true=电视剧 1、缺失=分类中 3）。与真实同步数据同构（DoubanDims
 *  投影），聚合/派生走 facet-engine 真路径，「分类中」节点的出现与其下
 *  国家→类型叶全部由真实构建逻辑产出；date DESC 排序契约照做 */
const DEMO_MOVIE_DIMS: DoubanDims = {
  user: 'demo',
  category: 'movie',
  status: 'collect',
  total: 6,
  items: [
    {
      id: 'demo-1',
      date: '2026-10-30',
      dims: { countries: ['中国大陆'], genres: ['纪录片'] },
    },
    {
      id: 'demo-2',
      date: '2026-10-28',
      dims: { countries: ['中国大陆'], genres: ['剧情'] },
    },
    {
      id: 'demo-3',
      date: '2026-10-25',
      dims: { countries: ['美国'], genres: ['科幻'] },
    },
    {
      id: 'demo-4',
      date: '2026-10-20',
      dims: { is_tv: true, countries: ['中国大陆'], genres: ['剧情'] },
    },
    {
      id: 'demo-5',
      date: '2026-10-18',
      dims: { is_tv: false, countries: ['日本'], genres: ['动画'] },
    },
    {
      id: 'demo-6',
      date: '2026-10-15',
      dims: { is_tv: false, countries: ['美国'], genres: ['动作'] },
    },
  ],
}

/** 系统减弱动态效果实时值（matchMedia 映射系统无障碍设置：Windows
 *  动画效果关闭/RDP 远程会话、macOS 减弱动态效果均报 reduce）——变更
 *  实时刷新，便于在系统设置页与演示页间对照 */
function usePrefersReducedMotion() {
  const [reduced, setReduced] = useState(
    () => window.matchMedia('(prefers-reduced-motion: reduce)').matches,
  )
  useEffect(() => {
    const mq = window.matchMedia('(prefers-reduced-motion: reduce)')
    const onChange = () => setReduced(mq.matches)
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [])
  return reduced
}

/** 计算样式 transform 矩阵 → 旋转角（0-359）：getComputedStyle 输出
 *  matrix(a, b, …)，角 = atan2(b, a)；none = 0°（无变换即初始角）。
 *  首元素带 "matrix(" 前缀须先剥，直接 split parseFloat 会得 NaN */
function readAngle(el: Element): number | null {
  const transform = getComputedStyle(el).transform
  if (transform === 'none') return 0
  const m = transform.match(/^matrix\(([-\d.eE+]+), ([-\d.eE+]+)/)
  if (!m) return null
  const deg = (Math.atan2(parseFloat(m[2]), parseFloat(m[1])) * 180) / Math.PI
  return (Math.round(deg) + 360) % 360
}

type SpinProbe = {
  /** 树内图标当前角度（0-359；null=节点不在 DOM） */
  angle: number | null
  /** 实测角速度（°/s，带方向；null=无相邻两拍可算） */
  rate: number | null
  /** 相邻两拍角度在推进（动画真实转动，而非仅样式声明） */
  spinning: boolean
  /** document.timeline 在推进（页面动画时钟活着；冻结的窗口/被节流的
   *  webview 里恒停，此时一切 CSS 动画都不动） */
  clockAlive: boolean
}

/** 旋转探针：定时采样树容器内 .facet-tree__icon-spin 的计算样式角度与
 *  文档时间线，产出「是否真的在转」的客观读数——人眼之外的平台无关
 *  判据（采样用 setInterval 而非 rAF：rAF 在时钟冻结的环境不触发，
 *  探针自身不能依赖被测的时钟） */
function useSpinProbe(treeRef: RefObject<HTMLDivElement | null>) {
  const [probe, setProbe] = useState<SpinProbe>({
    angle: null,
    rate: null,
    spinning: false,
    clockAlive: true,
  })
  useEffect(() => {
    let last: { angle: number | null; t: number } | null = null
    const sample = () => {
      const icon = treeRef.current?.querySelector('.facet-tree__icon-spin')
      const angle = icon ? readAngle(icon) : null
      // currentTime 类型是 CSSNumberish，Number() 收敛（实际恒为毫秒数）
      const now = Number(document.timeline.currentTime ?? 0)
      let rate: number | null = null
      if (last && angle !== null && last.angle !== null) {
        // 回绕安全差值：归到 -180~180，1.6s/圈在采样间隔内步进 < 180°
        const delta = ((angle - last.angle + 540) % 360) - 180
        const dt = now - last.t
        rate = dt > 0 ? Math.round((delta / dt) * 1000) : 0
      }
      const clockAlive = last ? now > last.t : true
      setProbe({
        angle,
        rate,
        spinning: rate !== null && Math.abs(rate) >= 5,
        clockAlive,
      })
      last = { angle, t: now }
    }
    sample()
    const id = window.setInterval(sample, 250)
    return () => window.clearInterval(id)
  }, [treeRef])
  return probe
}

/** 旋转演示页（`#/spin-demo` URL 直达，不入导航）：专用于展示与诊断
 *  「分类中」节点的刷新图标旋转。三件事：① 真实组件语境——FacetTree
 *  吃演示数据，「分类中」行一级可见（is_tv 缺失是富集暂态，图标持续
 *  转动示意流水线进行中）；② 尺寸/时长变体；③ 环境诊断——reduce
 *  实时值 + 时钟存活 + 实测角速度 + 强制播放判别（Windows 客户端上
 *  「默认静止、强制转动」= 系统 prefers-reduced-motion 关停实锤，
 *  而非动画失效） */
export function SpinDemoPage() {
  const navigate = useNavigate()
  const treeRef = useRef<HTMLDivElement>(null)
  const reduced = usePrefersReducedMotion()
  const probe = useSpinProbe(treeRef)
  const [forced, setForced] = useState(false)
  const [expandedTags, setExpandedTags] = useState<string[]>([])

  // 结论判读（诊断面板的出口）：转动优先，其次归因 reduce，再归因时钟
  // 冻结；时钟活着且未 reduce 却静止 = 样式被覆盖/丢失（理论不出现）
  const verdict = probe.spinning
    ? '转动中'
    : reduced
      ? '系统减弱动态效果关停（有意无障碍行为）'
      : probe.clockAlive
        ? '静止且时钟在走（样式未生效？）'
        : '页面时钟冻结（窗口不可见或被节流）'

  return (
    <MiniSiteTemplate
      contentClassName={forced ? 'spin-demo spin-demo--forced' : 'spin-demo'}
      headline="刷新图标，旋转演示"
      pageTitle="旋转演示"
      siteName="微墨·豆瓣"
      subheadline="真组件，真语境，旋转状态直读"
      topBarActions={
        <TextButton
          onClick={() => navigate('/')}
          type="button"
          variant="ghost"
        >
          返回首页
        </TextButton>
      }
    >
      <section className="spin-demo__section">
        <h2 className="spin-demo__section-title">树行语境（真实组件）</h2>
        <div
          className={getCardSurfaceClassName('spin-demo__tree')}
          ref={treeRef}
        >
          {/* 演示数据走真实构建链：形态一级行直接可见（无需展开），
              「分类中」图标即生产环境同款旋转；点击行不导航（onPathSelect
              空实现，受控 path 恒空选中态不动） */}
          <FacetTree
            activeCategory={undefined}
            dimsByCategory={{ movie: DEMO_MOVIE_DIMS }}
            expandedTags={expandedTags}
            onExpandedTagsChange={setExpandedTags}
            onPathSelect={() => {}}
            path={[]}
          />
        </div>
        <p className="spin-demo__variant-note">
          演示数据构造：is_tv 待富集条目归「分类中」，富集完成后节点随之
          消失（暂态语义同生产）
        </p>
      </section>

      <section className="spin-demo__section">
        <h2 className="spin-demo__section-title">尺寸与时长</h2>
        <div className={getCardSurfaceClassName('spin-demo__variant-row')}>
          <span className="spin-demo__variant">
            <RefreshCw aria-hidden className="facet-tree__icon-spin" size={16} />
            16
          </span>
          <span className="spin-demo__variant">
            <RefreshCw aria-hidden className="facet-tree__icon-spin" />
            24（生产）
          </span>
          <span className="spin-demo__variant">
            <RefreshCw aria-hidden className="facet-tree__icon-spin" size={40} />
            40
          </span>
          <span className="spin-demo__variant">
            <RefreshCw
              aria-hidden
              className="spin-demo__spin spin-demo__spin--fast"
            />
            0.8s
          </span>
          <span className="spin-demo__variant">
            <RefreshCw
              aria-hidden
              className="spin-demo__spin spin-demo__spin--slow"
            />
            3.2s
          </span>
        </div>
        <p className="spin-demo__variant-note">
          尺寸档复用生产 class（facet-tree__icon-spin，1.6s/圈），时长档
          同一 keyframes 换速；全部尊重系统减弱动态效果
        </p>
      </section>

      <section className="spin-demo__section">
        <h2 className="spin-demo__section-title">环境诊断</h2>
        <div className={getCardSurfaceClassName('spin-demo__probe')}>
          <div className="spin-demo__probe-row">
            <span className="spin-demo__probe-label">减弱动态效果</span>
            <span className="spin-demo__probe-value">
              {reduced ? '开启（reduce）' : '关闭'}
            </span>
          </div>
          <div className="spin-demo__probe-row">
            <span className="spin-demo__probe-label">动画时钟</span>
            <span className="spin-demo__probe-value">
              {probe.clockAlive ? '走动中' : '冻结'}
            </span>
          </div>
          <div className="spin-demo__probe-row">
            <span className="spin-demo__probe-label">图标角度</span>
            <span className="spin-demo__probe-value">
              {probe.angle === null
                ? '节点不在页面'
                : `${probe.angle}°${
                    probe.rate !== null ? ` · ${probe.rate}°/s` : ''
                  }`}
            </span>
          </div>
          <div className="spin-demo__probe-row">
            <span className="spin-demo__probe-label">运行环境</span>
            <span className="spin-demo__probe-value">
              {IN_TAURI_APP ? '桌面 App 壳' : '浏览器'} ·{' '}
              {navigator.platform || '未知平台'}
            </span>
          </div>
          <div className="spin-demo__probe-row">
            <span className="spin-demo__probe-label">结论</span>
            <span className="spin-demo__probe-value spin-demo__probe-verdict">
              {verdict}
            </span>
          </div>
        </div>

        <div className={getCardSurfaceClassName('spin-demo__force')}>
          <span className="spin-demo__force-name">
            <span className="spin-demo__force-title">强制播放</span>
            <span className="spin-demo__force-note">
              无视系统减弱动态效果驱动页内全部旋转元素，仅作判别工具
            </span>
          </span>
          <LabeledSwitch
            ariaLabel="强制播放"
            checked={forced}
            labelOff="关"
            labelOn="开"
            onCheckedChange={(checked) => setForced(checked)}
          />
        </div>

        <div className={getCardSurfaceClassName('spin-demo__probe')}>
          <table className="spin-demo__verdict-table">
            <thead>
              <tr>
                <th>现象</th>
                <th>结论</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>转动中</td>
                <td>环境正常，动画按 1.6s/圈推进</td>
              </tr>
              <tr>
                <td>静止 + 减弱动态效果开启</td>
                <td>
                  系统无障碍设置关停：Windows 动画效果关闭、性能选项最佳
                  性能或 RDP 远程会话，macOS 减弱动态效果
                </td>
              </tr>
              <tr>
                <td>静止 + 时钟冻结</td>
                <td>窗口不可见或 webview 被节流，非样式问题</td>
              </tr>
              <tr>
                <td>默认静止，强制播放后转动</td>
                <td>关停来自系统 reduce 实锤（开启开关即恢复可判）</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </MiniSiteTemplate>
  )
}
