import {
  HashRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router'
import type { SiteNavigationItem } from 'weimo-ui-template/components/site-template'
import { SiteTemplate } from 'weimo-ui-template/components/site-template'

import { DoubanUserProvider } from './site/douban-user-provider'
import {
  DEFAULT_MARKS_PATH,
  MARKS_NAVIGATION_GROUPS,
  resolveMarksView,
} from './site/navigation'
import {
  DEMO_MARK_CARD_PATH,
  MarkCardDemoPage,
} from './site/pages/mark-card-demo'
import { MarksPage } from './site/pages/marks-page'
import { SidebarHeader } from './site/sidebar-header'

/** 从 /:category 路径解析导航高亮项与顶栏标题 */
function useMarksViewFromLocation() {
  const { pathname } = useLocation()
  const [, category] = pathname.split('/')

  return resolveMarksView(category)
}

function SiteShell() {
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const view = useMarksViewFromLocation()
  // 演示页不在主导航，顶栏标题单独给
  const isDemo = pathname === DEMO_MARK_CARD_PATH

  function handleNavigate(item: SiteNavigationItem) {
    navigate(item.href.replace(/^#/, ''))
  }

  return (
    <SiteTemplate
      activeItemId={view?.activeItemId}
      navigationGroups={MARKS_NAVIGATION_GROUPS}
      onNavigate={handleNavigate}
      pageTitle={isDemo ? 'MarkCard 演示' : view?.pageTitle}
      showSidebarCloseButton={false}
      siteName="weimo-douban"
      sidebarHeader={<SidebarHeader />}
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
