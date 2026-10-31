import { HashRouter, Navigate, Route, Routes, useLocation, useSearchParams } from 'react-router'
import { SiteTemplate } from 'weimo-ui-template/components/site-template'
import { TagBread } from 'weimo-ui-tagtree/components/tag-bread'

import { DoubanUserProvider } from './site/douban-user-provider'
import {
  facetPathKey,
  facetValueLabel,
  parseFacetPath,
} from './site/facet-model'
import {
  CATEGORY_NOUNS,
  DEFAULT_MARKS_PATH,
  DEFAULT_MOVIE_KIND,
  moviePageTitle,
  resolveMarksView,
} from './site/navigation'
import { TopBarExport } from './site/marks-export'
import { MarksPage } from './site/pages/marks-page'
import { SettingsPage } from './site/pages/settings-page'
import { WelcomePage } from './site/pages/welcome-page'
import { TopBarSearch } from './site/search-dialog'
import { SiteSettingsProvider } from './site/site-settings-provider'
import { SiteSidebar } from './site/site-sidebar'

/** 从 /:category 路径解析导航高亮项与顶栏标题 */
function useMarksViewFromLocation() {
  const { pathname } = useLocation()
  const [, category] = pathname.split('/')

  return resolveMarksView(category)
}

/** 顶栏前置区：真实类别页常显标签面包屑。图书/音乐为「类别名/步骤
 *  值…」（无筛选单段类别名，page 级不可点）；movie 无类别段——首步
 *  形态值（电影/电视剧）即根段（真一级，无「影视」聚合名；裸 /movie
 *  已被 MarksRoute 重定向补形态步，理论必有）。点中间级=截断到该步
 *  （movie 根段点击=截到形态单步，图书/音乐根级点击=清空回全量）；
 *  all 返回 undefined，回落模板默认的 pageTitle 标签（pageTitle
 *  始终驱动 document.title） */
function useFacetBreadcrumb() {
  const { pathname } = useLocation()
  const [searchParams, setSearchParams] = useSearchParams()
  const view = resolveMarksView(pathname.split('/')[1])
  if (!view || view.category === 'all') return undefined
  const category = view.category
  const path = parseFacetPath(category, searchParams.get('facet'))
  const stepLabels = path.map((step) =>
    facetValueLabel(category, step.dim, step.value),
  )
  const tag = (
    category === 'movie'
      ? stepLabels
      : [CATEGORY_NOUNS[category], ...stepLabels]
  ).join('/')
  if (!tag) return undefined

  return (
    <TagBread
      onSelect={(sub) => {
        // sub 为截至该级的路径，kept=保留的步骤数：图书/音乐首级是
        // 类别名 kept=0=清空；movie 首级即形态步 kept=1（值可能含 /
        // 的边缘场景按段数截断）
        const kept = sub.split('/').length - 1
        setSearchParams(
          kept > 0 ? { facet: facetPathKey(path.slice(0, kept)) } : {},
        )
      }}
      tag={tag}
    />
  )
}

/** /:category 路由外壳：movie 的电影/电视剧是真一级——无形态步
 *  （is_tv）的路径（裸 /movie 或手改 URL 缺步）重定向规范为补默认
 *  形态前置步、保留其余步骤（replace 不留历史），「影视」聚合页随之
 *  不复存在 */
function MarksRoute() {
  const { pathname } = useLocation()
  const [searchParams] = useSearchParams()
  if (pathname.split('/')[1] === 'movie') {
    const path = parseFacetPath('movie', searchParams.get('facet'))
    if (!path.some((step) => step.dim === 'is_tv')) {
      const canonical = facetPathKey([
        { dim: 'is_tv', value: DEFAULT_MOVIE_KIND },
        ...path,
      ])
      return (
        <Navigate
          replace
          to={`/movie?facet=${encodeURIComponent(canonical)}`}
        />
      )
    }
  }
  return <MarksPage />
}

function SiteShell() {
  const [searchParams] = useSearchParams()
  const { pathname } = useLocation()
  const view = useMarksViewFromLocation()
  const topBarLeading = useFacetBreadcrumb()
  // 设置页不在 marks 视图解析范围内，单独给标题（pageTitle 始终驱动
  // document.title）；movie 标题随形态（真一级，无「影视」聚合名；形态
  // 步由 MarksRoute 保证存在，find 不到仅重定向前一帧的兜底）；其余
  // 类别/全部标记原样
  const pageTitle = pathname.split('/')[1] === 'settings'
    ? '设置'
    : view && view.category === 'movie'
      ? moviePageTitle(
          parseFacetPath('movie', searchParams.get('facet')).find(
            (step) => step.dim === 'is_tv',
          )?.value,
        )
      : view?.pageTitle

  // 根路径 = 欢迎页（营销页）：自带 MiniSiteTemplate 独立壳（无侧边栏、
  // hero 标题区），不套 SiteTemplate 应用壳。分支置于全部 hooks 之后——
  // 提前 return 会令 / 与应用页两分支 hook 数不一致，违反 Rules of Hooks
  if (pathname === '/') return <WelcomePage />

  return (
    <SiteTemplate
      pageTitle={pageTitle}
      showSidebarCloseButton={false}
      // 顶栏右侧主题切换让位给搜索+导出入口（主题仍随系统 defaultTheme 同步 html.dark）
      showThemeToggle={false}
      siteName="微墨·豆瓣"
      sidebar={(api) => <SiteSidebar onClose={api.close} />}
      topBarActions={
        <>
          <TopBarExport />
          <TopBarSearch />
        </>
      }
      topBarLeading={topBarLeading}
    >
      <Routes>
        {/* 静态段优先于动态 /:category，react-router 按具体度排序匹配 */}
        <Route element={<SettingsPage />} path="/settings" />
        <Route element={<MarksRoute />} path="/:category" />
        <Route
          element={<Navigate replace to={DEFAULT_MARKS_PATH} />}
          path="*"
        />
      </Routes>
    </SiteTemplate>
  )
}

function App() {
  return (
    <DoubanUserProvider>
      <SiteSettingsProvider>
        <HashRouter>
          <SiteShell />
        </HashRouter>
      </SiteSettingsProvider>
    </DoubanUserProvider>
  )
}

export default App
