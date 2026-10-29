import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { BookOpen, Clapperboard, Music } from 'lucide-react'
import { useLocation, useNavigate, useSearchParams } from 'react-router'
import { TagTree, type TagTreeNode } from 'weimo-ui-tagtree/components/tag-tree'

import { fetchDoubanFacets, type DoubanFacets } from './api'
import { useDoubanUser } from './douban-user'
import { FacetTree } from './facet-tree'
import { parseFacetParam, type FacetSelection } from './facet-model'
import { SidebarDebugSwitch } from './sidebar-debug-switch'
import { SidebarHeader } from './sidebar-header'
import { SidebarStats } from './sidebar-stats'
import {
  CATEGORY_NOUNS,
  MARKS_CATEGORIES,
  resolveMarksView,
  type MarksCategory,
} from './navigation'
import './site-sidebar.css'

/** 类别导航行图标（TagTree no-action 纯导航，用法对齐
 *  biji-side-bar__nav 的样例导航树） */
const NAV_ICONS: Record<MarksCategory, ReactNode> = {
  book: <BookOpen aria-hidden="true" />,
  movie: <Clapperboard aria-hidden="true" />,
  music: <Music aria-hidden="true" />,
}

/** 类别导航节点：电影/图书/音乐一行一项（tag 即路由段） */
const navNodes: TagTreeNode[] = MARKS_CATEGORIES.map((category) => ({
  tag: category,
  label: CATEGORY_NOUNS[category],
  icon: NAV_ICONS[category],
}))

/** 站点侧边栏：整体接管模板 sidebar 插槽，布局对齐 weimo-biji 的
 *  biji-side-bar——身份栏固定在顶，滚动主体依次为标记统计区
 *  （StatGroup/Heatmap）→ 类别导航 → 分面筛选树（随当前类别）。
 *  分面选择经 URL ?facet=dim=value 与列表页共享（侧边栏写、列表页
 *  读）；类别导航不带参数跳转，切换类别天然清除筛选。
 *  onClose 供窄屏抽屉在选中后收起 */
export function SiteSidebar({ onClose }: { onClose: () => void }) {
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const [searchParams, setSearchParams] = useSearchParams()
  const { uid } = useDoubanUser()
  const category = resolveMarksView(pathname.split('/')[1])?.category
  const selection = category
    ? parseFacetParam(category, searchParams.get('facet'))
    : null

  // 分面聚合：随 uid/类别重取，与筛选选择无关；失败/未同步返回 null，
  // 树区静默隐藏（快照模式同 marks-page，渲染期回退 loading）
  const facetsKey = uid && category ? `${uid}|${category}` : null
  const [facetsSnapshot, setFacetsSnapshot] = useState<{
    key: string | null
    loading: boolean
    data: DoubanFacets | null
  }>({ key: null, loading: false, data: null })
  if (facetsKey !== facetsSnapshot.key) {
    setFacetsSnapshot({ key: facetsKey, loading: true, data: null })
  }
  useEffect(() => {
    if (!facetsKey || !uid || !category) return
    let cancelled = false
    fetchDoubanFacets(uid, category).then((data) => {
      if (!cancelled) {
        setFacetsSnapshot({ key: facetsKey, loading: false, data })
      }
    })
    return () => {
      cancelled = true
    }
  }, [facetsKey, uid, category])

  function handleNavSelect(tag: string) {
    // 不带查询参数导航，类别切换天然清除分面筛选
    navigate(`/${tag}`)
    onClose()
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  function handleFacetSelect(next: FacetSelection | null) {
    setSearchParams(
      next ? { facet: `${next.dim}=${next.value}` } : {},
    )
  }

  // 筛选树区：未登录/无类别（演示页）不渲染；加载中给占位行，
  // 失败/未同步整区隐藏
  const facetsData = facetsSnapshot.data
  const facetSection = !uid || !category ? null : (
    <section aria-label="分面筛选" className="site-sidebar__facet">
      <div className="site-sidebar__section-hint">筛选</div>
      {facetsData ? (
        <FacetTree
          category={category}
          facets={facetsData}
          onSelect={handleFacetSelect}
          selection={selection}
        />
      ) : facetsSnapshot.loading ? (
        <div className="site-sidebar__status" role="status">
          筛选加载中…
        </div>
      ) : null}
    </section>
  )

  return (
    <div className="site-sidebar">
      <SidebarHeader />
      <div className="site-sidebar__body">
        <SidebarStats />
        <nav aria-label="条目类别" className="site-sidebar__nav">
          <TagTree
            aria-label="条目类别"
            nodes={navNodes}
            onSelect={handleNavSelect}
            selectedTag={category}
            variant="no-action"
          />
        </nav>
        {facetSection}
      </div>
      <SidebarDebugSwitch />
    </div>
  )
}
