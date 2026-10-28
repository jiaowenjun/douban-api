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
  const view = useMarksViewFromLocation()

  function handleNavigate(item: SiteNavigationItem) {
    navigate(item.href.replace(/^#/, ''))
  }

  return (
    <SiteTemplate
      activeItemId={view?.activeItemId}
      navigationGroups={MARKS_NAVIGATION_GROUPS}
      onNavigate={handleNavigate}
      pageTitle={view?.pageTitle}
      siteName="weimo-douban"
      sidebarHeader={<SidebarHeader siteName="weimo-douban" />}
    >
      <Routes>
        <Route element={<Navigate replace to={DEFAULT_MARKS_PATH} />} path="/" />
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
