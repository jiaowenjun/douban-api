import {
  Apple,
  ArrowRight,
  Globe,
  Image as ImageIcon,
  Monitor,
  RefreshCw,
  Server,
  Sparkles,
} from 'lucide-react'
import { useNavigate } from 'react-router'
import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { TextButton } from 'weimo-ui-core/components/text-button'
import { MiniSiteTemplate } from 'weimo-ui-template/components/mini-site-template'

import { useDoubanUser } from '../douban-user'
import darkThemeShot from './assets/dark-theme.png'
import facetFilterShot from './assets/facet-filter.png'
import marksOverviewShot from './assets/marks-overview.png'
import ratingFilterShot from './assets/rating-filter.png'
import sidebarStatsShot from './assets/sidebar-stats.png'
import './welcome-page.css'

/** 次要卖点卡：图标 + 标题 + 一句话描述，纯文案网格 */
const HIGHLIGHTS = [
  {
    icon: Globe,
    title: '免登录抓取',
    description: '直读豆瓣公开收藏页，无需账号与 Cookie，标记与短评一并入库。',
  },
  {
    icon: RefreshCw,
    title: '自动增量同步',
    description: '订阅轮询 + 维度缓存，只抓变化的部分；全局限速与熔断保护豆瓣侧。',
  },
  {
    icon: Sparkles,
    title: '条目元信息富集',
    description: '年份 / 导演 / 主演 / 类型自动补全，跨用户共享缓存，越用越快。',
  },
  {
    icon: ImageIcon,
    title: '封面代理防裂图',
    description: '绕开豆瓣 CDN 防盗链并磁盘缓存，列表封面即刷即出，告别裂图。',
  },
  {
    icon: Server,
    title: '单二进制自托管',
    description: 'Rust 编译为单个可执行文件，一个数据目录就是全部状态，随处部署。',
  },
  {
    icon: Monitor,
    title: 'macOS 菜单栏 App',
    description: 'Tauri 全栈封装：首启快照种子、菜单栏常驻、GitHub Releases 自动更新。',
  },
] as const

/** 桌面 App 版本与下载地址：版本号与 app/tauri.conf.json 的 version 保持
 *  同步；dmg 资产名是 release.sh 统一的 ASCII 前缀（gh 上传会剥掉 CJK
 *  文件名，此处的 latest/download 直链按产物名精确匹配） */
const APP_VERSION = '0.4.0'
const APP_DMG_URL = `https://github.com/jiaowenjun/weimo-douban/releases/latest/download/weimo-douban_${APP_VERSION}_aarch64.dmg`
const APP_RELEASES_URL = 'https://github.com/jiaowenjun/weimo-douban/releases'

/** 主卖点小节：图文行（截图 + 标题描述），相邻小节左右交替 */
const FEATURES = [
  {
    shot: sidebarStatsShot,
    shotAlt: '侧边栏统计与活动热力图：影视、图书、音乐三类计数与近十三周标记热力格',
    tall: true,
    title: '全景统计，一目了然',
    description:
      '三类计数与十三周活动热力图常驻侧栏，你的观看节奏一眼可读；标记卡片保留标记日期，我的评分与豆瓣评分双星对照。',
  },
  {
    shot: facetFilterShot,
    shotAlt: '分面筛选树：电影按中国大陆筛选，顶栏面包屑与列表联动',
    tall: false,
    title: '分面筛选树，一步直达',
    description:
      '类别 → 维度 → 值三级下钻：地区、类型、年代、导演、主演……点开即筛；筛选路径化作顶栏面包屑，随时回跳任意层级。',
  },
  {
    shot: ratingFilterShot,
    shotAlt: '全部标记视图按我的五星评分筛选的结果列表',
    tall: false,
    title: '评分快捷筛选',
    description:
      '我的五星标记、四星标记、三分及以下——「全部标记」视图按你的评分一键分档，好片烂片各归其位。',
  },
  {
    shot: darkThemeShot,
    shotAlt: '深色主题下的标记列表：展开卡片与折叠卡片混排',
    tall: false,
    title: '明暗双主题',
    description:
      '浅色清爽、深色护眼，默认跟随系统自动切换；上图即应用实拍，两种主题同一套组件渲染，观感一致。',
  },
] as const

/** 欢迎页（营销页）：`/` 根路径的门面。MiniSiteTemplate 无侧边栏单页壳
 *  （hero 标题区 + 全宽内容），展示卖点与真实界面截图；CTA 按登录态换
 *  文案进入标记列表。截图为 1280×850 视口实拍，经 import 引用随生产
 *  base（/douban/）重写资源地址 */
export function WelcomePage() {
  const navigate = useNavigate()
  const { uid } = useDoubanUser()

  return (
    <MiniSiteTemplate
      contentClassName="welcome-page"
      headline="把你的豆瓣书影音，搬回自己手里"
      pageTitle="首页"
      siteName="微墨·豆瓣"
      subheadline="免登录同步你的公开标记与短评，自动补全条目元信息，热力图与分面筛选树帮你重新发现十年积攒——自托管 Web，附 macOS 菜单栏桌面 App。"
      topBarLeading={<span className="welcome-page__brand">微墨·豆瓣</span>}
    >
      <div className="welcome-page__cta">
        <TextButton
          onClick={() => navigate('/movie')}
          type="button"
        >
          {uid ? '进入我的标记' : '进入标记列表'}
          <ArrowRight aria-hidden size={16} />
        </TextButton>
        <TextButton
          onClick={() => navigate('/all')}
          type="button"
          variant="ghost"
        >
          查看全部标记
        </TextButton>
      </div>

      <figure className={getCardSurfaceClassName('welcome-page__shot')}>
        <img
          alt="标记列表页全景：侧边栏统计与筛选树、顶栏面包屑、封面与双星评分的标记卡片"
          src={marksOverviewShot}
        />
        <figcaption>
          标记列表：封面、双星评分、条目元信息与富文本短评，逐卡折叠展开
        </figcaption>
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
            <img alt={feature.shotAlt} src={feature.shot} />
          </figure>
          <div className="welcome-page__feature-text">
            <h3>{feature.title}</h3>
            <p>{feature.description}</p>
          </div>
        </section>
      ))}

      <section className="welcome-page__section">
        <h2 className="welcome-page__section-title">为什么选微墨·豆瓣</h2>
        <div className="welcome-page__grid">
          {HIGHLIGHTS.map(({ icon: Icon, title, description }) => (
            <div
              className={getCardSurfaceClassName('welcome-page__card')}
              key={title}
            >
              <Icon aria-hidden className="welcome-page__card-icon" size={20} />
              <div className="welcome-page__card-title">{title}</div>
              <p className="welcome-page__card-desc">{description}</p>
            </div>
          ))}
        </div>
      </section>

      <section className="welcome-page__section">
        <div className={getCardSurfaceClassName('welcome-page__download')}>
          <div className="welcome-page__download-text">
            <div className="welcome-page__download-title">
              <Apple aria-hidden size={20} />
              macOS 客户端
            </div>
            <p className="welcome-page__download-desc">
              Apple Silicon（aarch64）· v{APP_VERSION} · 首启自动拉取全库快照种子，已装旧版经 GitHub Releases 自动更新。
            </p>
          </div>
          <div className="welcome-page__download-actions">
            {/* 借用 text-button 组件类渲染真链接：下载类 CTA 需要真实
             * href（右键另存/新标签直下），dmg 直链由浏览器下载不离开本页 */}
            <a className="text-button" href={APP_DMG_URL}>
              下载 dmg
            </a>
            <a
              className="text-button"
              data-variant="ghost"
              href={APP_RELEASES_URL}
              rel="noreferrer"
              target="_blank"
            >
              全部版本
            </a>
          </div>
        </div>
      </section>

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
