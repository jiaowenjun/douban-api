import { ArrowRight } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router'
import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { TextButton } from 'weimo-ui-core/components/text-button'
import { MiniSiteTemplate } from 'weimo-ui-template/components/mini-site-template'

import { useDoubanUser } from '../douban-user'
import appIcon from './assets/app-icon.svg'
import facetFilterDark from './assets/facet-filter-dark.png'
import facetFilterLight from './assets/facet-filter-light.png'
import marksOverviewDark from './assets/marks-overview-dark.png'
import marksOverviewLight from './assets/marks-overview-light.png'
import ratingFilterDark from './assets/rating-filter-dark.png'
import ratingFilterLight from './assets/rating-filter-light.png'
import sidebarStatsDark from './assets/sidebar-stats-dark.png'
import sidebarStatsLight from './assets/sidebar-stats-light.png'
import './welcome-page.css'

/** 主卖点小节：图文行（当前主题截图 + 标题描述），相邻小节左右交替 */
const FEATURES = [
  {
    shotAlt: '侧边栏统计与活动热力图：影视、图书、音乐三类计数与近十三周标记热力格',
    shotDark: sidebarStatsDark,
    shotLight: sidebarStatsLight,
    tall: true,
    title: '全景统计，一目了然',
    description:
      '三类计数与十三周活动热力图常驻侧栏，你的观看节奏一眼可读；标记卡片保留标记日期，我的评分与豆瓣评分双星对照。',
  },
  {
    shotAlt: '分面筛选树：电影按中国大陆筛选，顶栏面包屑与列表联动',
    shotDark: facetFilterDark,
    shotLight: facetFilterLight,
    tall: false,
    title: '分面筛选树，一步直达',
    description:
      '类别 → 维度 → 值三级下钻：地区、类型、年代、导演、主演……点开即筛；筛选路径化作顶栏面包屑，随时回跳任意层级。',
  },
  {
    shotAlt: '全部标记视图按我的五星评分筛选的结果列表',
    shotDark: ratingFilterDark,
    shotLight: ratingFilterLight,
    tall: false,
    title: '评分快捷筛选',
    description:
      '我的五星标记、四星标记、三分及以下——「全部标记」视图按你的评分一键分档，好片烂片各归其位。',
  },
] as const

/** 桌面 App 版本与 dmg 直链（hero CTA 次按钮）：版本号与
 *  app/tauri.conf.json 的 version 保持同步；dmg 资产名是 release.sh
 *  统一的 ASCII 前缀（gh 上传会剥掉 CJK 文件名，此处的 latest/download
 *  直链按产物名精确匹配） */
const APP_VERSION = '0.4.0'
const APP_DMG_URL = `https://github.com/jiaowenjun/weimo-douban/releases/latest/download/weimo-douban_${APP_VERSION}_aarch64.dmg`

/** 当前实际生效主题是否为深色：模板主题（跟随系统或顶栏手动切换）最终
 *  都同步到 html.dark 类，观测根类名即唯一事实源——MutationObserver 监听
 *  class 变化，系统主题增删与手动切换均实时驱动截图换图（亮主题只显亮
 *  色截图、暗主题只显暗色截图，页面永不混排两套主题的实拍） */
function useAppliedDarkTheme(): boolean {
  const [dark, setDark] = useState(() =>
    document.documentElement.classList.contains('dark'),
  )
  useEffect(() => {
    const root = document.documentElement
    const observer = new MutationObserver(() => {
      setDark(root.classList.contains('dark'))
    })
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
    return () => observer.disconnect()
  }, [])
  return dark
}

/** Apple 官方标志（Font Awesome「apple」品牌字形，CC BY 4.0）：lucide 的
 *  Apple 是水果造型非品牌标志，故内联此路径；fill=currentColor 随按钮
 *  文字色走，明暗主题与实心/ghost 变体均自适应——img 引 SVG 无法继承
 *  文字色（黑色 logo 在实心主按钮上隐形），只能内联 */
function AppleLogo({ size = 16 }: { size?: number }) {
  return (
    <svg
      aria-hidden
      fill="currentColor"
      height={size}
      viewBox="0 0 24 24"
      width={size}
    >
      <path d="M18.71 19.5C17.88 20.74 17 21.95 15.66 21.97C14.32 22 13.89 21.18 12.37 21.18C10.84 21.18 10.37 21.95 9.09997 22C7.78997 22.05 6.79997 20.68 5.95997 19.47C4.24997 17 2.93997 12.45 4.69997 9.39C5.56997 7.87 7.12997 6.91 8.81997 6.88C10.1 6.86 11.32 7.75 12.11 7.75C12.89 7.75 14.37 6.68 15.92 6.84C16.57 6.87 18.39 7.1 19.56 8.82C19.47 8.88 17.39 10.1 17.41 12.63C17.44 15.65 20.06 16.66 20.09 16.67C20.06 16.74 19.67 18.11 18.71 19.5ZM13 3.5C13.73 2.67 14.94 2.04 15.94 2C16.07 3.17 15.6 4.35 14.9 5.19C14.21 6.04 13.07 6.7 11.95 6.61C11.8 5.46 12.36 4.26 13 3.5Z" />
    </svg>
  )
}

/** 欢迎页（营销页）：`/` 根路径的门面。MiniSiteTemplate 无侧边栏单页壳
 *  （hero 标题区 + 全宽内容），展示卖点与真实界面截图；CTA 按登录态换
 *  文案进入标记列表。截图按当前生效主题成对提供（*-light/*-dark 各一套
 *  实拍，1280×850），经 import 引用随生产 base（/douban/）重写资源地址 */
export function WelcomePage() {
  const navigate = useNavigate()
  const { uid } = useDoubanUser()
  const dark = useAppliedDarkTheme()

  return (
    <MiniSiteTemplate
      contentClassName="welcome-page"
      headline="豆瓣书影音，搬回自己手里"
      pageTitle="首页"
      siteName="微墨·豆瓣"
      subheadline="免登录同步，自托管保存，亮暗主题自适应"
      topBarLeading={
        <span className="welcome-page__brand">
          {/* alt 置空：紧邻的品牌文字已传达名称，图标纯装饰 */}
          <img alt="" className="welcome-page__brand-icon" src={appIcon} />
          微墨·豆瓣
        </span>
      }
    >
      <div className="welcome-page__cta">
        <TextButton
          onClick={() => navigate('/all')}
          type="button"
        >
          {uid ? '进入我的标记' : '进入标记列表'}
          <ArrowRight aria-hidden size={16} />
        </TextButton>
        {/* 借用 text-button 组件类渲染真链接：下载 CTA 需要真实
         * href（右键另存/新标签直下），dmg 直链由浏览器下载不离开本页；
         *  无 data-variant 即默认实心，与左侧进入按钮同款 */}
        <a className="text-button" href={APP_DMG_URL}>
          <AppleLogo size={16} />
          Mac 客户端
        </a>
      </div>

      <figure
        className={getCardSurfaceClassName(
          'welcome-page__shot welcome-page__shot--hero',
        )}
      >
        <img
          alt="标记列表页全景：侧边栏统计与筛选树、顶栏面包屑、封面与双星评分的标记卡片"
          src={dark ? marksOverviewDark : marksOverviewLight}
        />
      </figure>

      {FEATURES.map((feature) => (
        <section className="welcome-page__feature" key={feature.title}>
          <figure
            className={getCardSurfaceClassName(
              feature.tall
                ? 'welcome-page__shot welcome-page__shot--tall'
                : 'welcome-page__shot',
            )}
          >
            <img
              alt={feature.shotAlt}
              src={dark ? feature.shotDark : feature.shotLight}
            />
          </figure>
          <div className="welcome-page__feature-text">
            <h3>{feature.title}</h3>
            <p>{feature.description}</p>
          </div>
        </section>
      ))}

      <footer className="welcome-page__footer">
        <span>开源仓库：</span>
        <a
          href="https://github.com/jiaowenjun/weimo-douban"
          rel="noreferrer"
          target="_blank"
        >
          GitHub · jiaowenjun/weimo-douban
        </a>
        <span> · 界面与组件基于 weimo-ui</span>
      </footer>
    </MiniSiteTemplate>
  )
}
