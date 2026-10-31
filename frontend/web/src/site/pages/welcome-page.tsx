import { ArrowRight } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router'
import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { FrostedIconButton } from 'weimo-ui-core/components/frosted-icon-button'
import { TextButton } from 'weimo-ui-core/components/text-button'
import { MiniSiteTemplate } from 'weimo-ui-template/components/mini-site-template'

import { useDoubanUser } from '../douban-user'
import appIcon from './assets/app-icon.svg'
import appIconWhite from './assets/app-icon-white.svg'
import marksOverviewDark from './assets/marks-overview-dark.png'
import marksOverviewLight from './assets/marks-overview-light.png'
import './welcome-page.css'

/** 桌面 App 版本与 dmg 直链（hero CTA 次按钮）：版本号与
 *  app/tauri.conf.json 的 version 保持同步；dmg 资产名是 release.sh
 *  统一的 ASCII 前缀（gh 上传会剥掉 CJK 文件名，此处的 latest/download
 *  直链按产物名精确匹配） */
const APP_VERSION = '0.4.0'
const APP_DMG_URL = `https://github.com/jiaowenjun/weimo-douban/releases/latest/download/weimo-douban_${APP_VERSION}_aarch64.dmg`
const REPO_URL = 'https://github.com/jiaowenjun/weimo-douban'

/** 当前实际生效主题是否为深色：模板主题最终都同步到 html.dark 类，观测
 *  根类名即唯一事实源——MutationObserver 监听 class 变化，系统主题增删
 *  实时驱动截图换图（亮主题只显亮色截图、暗主题只显暗色截图，页面永不
 *  混排两套主题的实拍）。顶栏主题切换按钮已让位给 GitHub 仓库入口
 *  （showThemeToggle=false），本页主题只随系统 */
function useAppliedDarkTheme(): boolean {
  const [dark, setDark] = useState(() =>
    document.documentElement.classList.contains('dark'),
  )
  useEffect(() => {
    const root = document.documentElement
    const sync = () => {
      setDark(root.classList.contains('dark'))
    }
    // 订阅时先补读一次当前值（订阅即同步初值的范式）：useState 初值在
    // render 阶段读，早于模板把 html.dark 同步进 DOM 的 effect（子先于
    // 父），系统暗色冷挂载会读到 false，且 observer 挂上时唯一一次类
    // 变化已错过——亮色截图就此常驻暗色页面；本 effect 执行时模板
    // effect 必已跑完，sync() 读到正确值，effect 顺序若反转则由
    // observer 兜底，两个方向都闭合
    sync()
    const observer = new MutationObserver(sync)
    observer.observe(root, { attributes: true, attributeFilter: ['class'] })
    return () => observer.disconnect()
  }, [])
  return dark
}

/** GitHub 标记（GitHub 官方 logo 路径）：lucide-react 1.x 已移除品牌图标，
 *  内联为 SVG；fill=currentColor 随按钮文字色走，明暗主题均自适应——同
 *  AppleLogo，img 引 SVG 无法继承文字色，只能内联（口径同
 *  weimo-ui-tagtree 演示页的 GitHubIcon） */
function GitHubIcon({ size = 24 }: { size?: number }) {
  return (
    <svg
      aria-hidden
      fill="currentColor"
      height={size}
      viewBox="0 0 24 24"
      width={size}
    >
      <path d="M12 .297c-6.63 0-12 5.373-12 12c0 5.303 3.438 9.8 8.205 11.385c.6.113.82-.258.82-.577c0-.285-.01-1.04-.015-2.04c-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729c1.205.084 1.838 1.236 1.838 1.236c1.07 1.835 2.809 1.305 3.495.998c.108-.776.417-1.305.76-1.605c-2.665-.3-5.466-1.332-5.466-5.93c0-1.31.465-2.38 1.235-3.22c-.135-.303-.54-1.523.105-3.176c0 0 1.005-.322 3.3 1.23c.96-.267 1.98-.399 3-.405c1.02.006 2.04.138 3 .405c2.28-1.552 3.285-1.23 3.285-1.23c.645 1.653.24 2.873.12 3.176c.765.84 1.23 1.91 1.23 3.22c0 4.61-2.805 5.625-5.475 5.92c.42.36.81 1.096.81 2.22c0 1.606-.015 2.896-.015 3.286c0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
    </svg>
  )
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
 *  （hero 标题区 + 全宽内容），CTA 按登录态换文案进入标记列表，主截图
 *  一张收尾。截图按当前生效主题成对提供（*-light/*-dark 各一套实拍，
 *  1280×960，4:3），经 import 引用随生产 base（/douban/）重写资源地址 */
export function WelcomePage() {
  const navigate = useNavigate()
  const { uid } = useDoubanUser()
  const dark = useAppliedDarkTheme()

  return (
    <MiniSiteTemplate
      contentClassName="welcome-page"
      headline="豆瓣书影音，一键备份"
      pageTitle="首页"
      siteName="微墨·豆瓣"
      showThemeToggle={false}
      subheadline="免登录，自托管，亮暗主题自适应"
      topBarActions={
        <FrostedIconButton
          aria-label="GitHub 仓库"
          onClick={() => window.open(REPO_URL, '_blank', 'noopener,noreferrer')}
          tooltip="GitHub 仓库"
        >
          <GitHubIcon />
        </FrostedIconButton>
      }
      topBarLeading={
        <span className="welcome-page__brand">
          {/* 黑白双稿随主题切换：亮色浓墨 / 暗色纸白，dark 来自
              useAppliedDarkTheme 对 html.dark 的观测（口径同下方主截图
              的双主题换图）；alt 置空：紧邻的品牌文字已传达名称，图标纯装饰 */}
          <img
            alt=""
            className="welcome-page__brand-icon"
            src={dark ? appIconWhite : appIcon}
          />
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
    </MiniSiteTemplate>
  )
}
