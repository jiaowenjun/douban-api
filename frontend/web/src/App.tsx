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
  resolveMarksView,
} from './site/navigation'
import {
  DEMO_MARK_CARD_PATH,
  MarkCardDemoPage,
} from './site/pages/mark-card-demo'
import { MarksPage } from './site/pages/marks-page'
import { SiteSidebar } from './site/site-sidebar'

/** 从 /:category 路径解析导航高亮项与顶栏标题 */
function useMarksViewFromLocation() {
  const { pathname } = useLocation()
  const [, category] = pathname.split('/')

  return resolveMarksView(category)
}

/** 顶栏前置区：真实类别页常显标签面包屑——无筛选单段类别名（page 级
 *  不可点），筛选激活时「类别名/步骤值…」（影视叶为 影视/电影/科幻
 *  三段，值走 facetValueLabel）；点中间级=截断到该步，点首级类别名=
 *  清除筛选回全量。all/演示页返回 undefined，回落模板默认的 pageTitle
 *  标签（pageTitle 始终驱动 document.title） */
function useFacetBreadcrumb() {
  const { pathname } = useLocation()
  const [searchParams, setSearchParams] = useSearchParams()
  const view = resolveMarksView(pathname.split('/')[1])
  if (!view || view.category === 'all') return undefined
  const category = view.category
  const path = parseFacetPath(category, searchParams.get('facet'))
  const noun = CATEGORY_NOUNS[category]

  return (
    <TagBread
      onSelect={(sub) => {
        // sub 为截至该级的路径（类别名 或 类别名/第一步值）：
        // 首级=清空，其余=截断到该步（值可能含 / 的边缘场景按段数截断）
        const kept = sub.split('/').length - 1
        setSearchParams(
          kept > 0 ? { facet: facetPathKey(path.slice(0, kept)) } : {},
        )
      }}
      tag={[
        noun,
        ...path.map((step) => facetValueLabel(category, step.dim, step.value)),
      ].join('/')}
    />
  )
}

function SiteShell() {
  const { pathname } = useLocation()
  const view = useMarksViewFromLocation()
  // 演示页不在主导航，顶栏标题单独给
  const isDemo = pathname === DEMO_MARK_CARD_PATH
  const topBarLeading = useFacetBreadcrumb()

  return (
    <SiteTemplate
      pageTitle={isDemo ? 'MarkCard 演示' : view?.pageTitle}
      showSidebarCloseButton={false}
      siteName="weimo-douban"
      sidebar={(api) => <SiteSidebar onClose={api.close} />}
      topBarLeading={topBarLeading}
    >
      <Routes>
        <Route element={<Navigate replace to={DEFAULT_MARKS_PATH} />} path="/" />
        <Route element={<MarkCardDemoPage />} path={DEMO_MARK_CARD_PATH} />
        <Route element={<MarksPage />} path="/:category" />
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
      <HashRouter>
        <SiteShell />
      </HashRouter>
    </DoubanUserProvider>
  )
}

export default App
