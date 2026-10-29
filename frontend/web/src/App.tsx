import { HashRouter, Navigate, Route, Routes, useLocation } from 'react-router'
import { SiteTemplate } from 'weimo-ui-template/components/site-template'

import { DoubanUserProvider } from './site/douban-user-provider'
import {
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

function SiteShell() {
  const { pathname } = useLocation()
  const view = useMarksViewFromLocation()
  // 演示页不在主导航，顶栏标题单独给
  const isDemo = pathname === DEMO_MARK_CARD_PATH

  return (
    <SiteTemplate
      pageTitle={isDemo ? 'MarkCard 演示' : view?.pageTitle}
      showSidebarCloseButton={false}
      siteName="weimo-douban"
      sidebar={(api) => <SiteSidebar onClose={api.close} />}
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
