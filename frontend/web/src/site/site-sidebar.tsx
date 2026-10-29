import { useEffect, useState } from 'react'
import { Grid2X2 } from 'lucide-react'
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
  ALL_MARKS_TAG,
  MARKS_CATEGORIES,
  resolveMarksView,
  type MarksCategory,
  type NavTarget,
} from './navigation'
import './site-sidebar.css'

/** 导航区节点：全部标记（书影音合并视图入口，图标对齐 biji-side-bar
 *  全部笔记的 Grid2X2；类别导航由筛选树一级根兼任，不在此重复） */
const navNodes: TagTreeNode[] = [
  { tag: ALL_MARKS_TAG, label: '全部标记', icon: <Grid2X2 aria-hidden="true" /> },
]

/** 站点侧边栏：整体接管模板 sidebar 插槽，布局对齐 weimo-biji 的
 *  biji-side-bar——身份栏固定在顶，滚动主体依次为标记统计区
 *  （StatGroup/Heatmap）→ 导航区（全部标记）→ 分面筛选树（一级
 *  类别根带图标，点行=跳该类别清筛选，兼任类别导航）。
 *  分面选择经 URL ?facet=dim=value 与列表页共享（侧边栏写、列表页
 *  读）；类别切换天然清除筛选。
 *  onClose 供窄屏抽屉在选中后收起 */
export function SiteSidebar({ onClose }: { onClose: () => void }) {
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const [searchParams, setSearchParams] = useSearchParams()
  const { uid } = useDoubanUser()
  const category = resolveMarksView(pathname.split('/')[1])?.category
  // 分面仅真实类别有维度（all 合并视图无分面）
  const realCategory = category !== undefined && category !== ALL_MARKS_TAG
    ? category
    : undefined
  const selection = realCategory
    ? parseFacetParam(realCategory, searchParams.get('facet'))
    : null

  // 分面聚合：三类别一次并行取，快照只随 uid 变不随路由变——树是
  // 三类别常驻结构，切类别不再重取；单类别失败/未同步该根不出现
  // （allSettled 容错，快照模式同 marks-page 渲染期回退 loading）
  const facetsKey = uid
  const [facetsSnapshot, setFacetsSnapshot] = useState<{
    key: string | null
    loading: boolean
    data: Partial<Record<MarksCategory, DoubanFacets>> | null
  }>({ key: null, loading: false, data: null })
  if (facetsKey !== facetsSnapshot.key) {
    setFacetsSnapshot({ key: facetsKey, loading: true, data: null })
  }
  useEffect(() => {
    if (!facetsKey) return
    let cancelled = false
    Promise.allSettled(
      MARKS_CATEGORIES.map((facetCategory) =>
        fetchDoubanFacets(facetsKey, facetCategory),
      ),
    ).then((results) => {
      if (cancelled) return
      const byCategory: Partial<Record<MarksCategory, DoubanFacets>> = {}
      MARKS_CATEGORIES.forEach((facetCategory, index) => {
        const result = results[index]
        if (result.status === 'fulfilled' && result.value) {
          byCategory[facetCategory] = result.value
        }
      })
      setFacetsSnapshot({ key: facetsKey, loading: false, data: byCategory })
    })
    return () => {
      cancelled = true
    }
  }, [facetsKey])

  // 展开态：类别切换时重置为「当前类别根展开」；用户手动展开的
  // 其他类别在同一类别会话内保留。选中叶子到维度的链路不必记录
  // ——TagTree 对选中祖先自动展开
  const [expandedTags, setExpandedTags] = useState<string[]>([])
  const [expandedCategory, setExpandedCategory] = useState<
    NavTarget | undefined | null
  >(null)
  if (category !== expandedCategory) {
    setExpandedCategory(category)
    // 全部标记节点无子树，目标为 all 时无需展开任何根
    setExpandedTags(
      category !== undefined && category !== ALL_MARKS_TAG ? [category] : [],
    )
  }

  /** 分面选择统一处理：目标=当前类别时改查询参数即生效（null 即清除
   *  筛选——叶子再点取消与一级导航节点点击重置都走这里）；目标为其他
   *  类别或「全部标记」（叶子或导航节点）跳路由，带值叶子一步带筛选、
   *  导航节点跳纯类别路由 */
  function handleFacetSelect(
    target: NavTarget,
    next: FacetSelection | null,
  ) {
    if (target === category) {
      setSearchParams(next ? { facet: `${next.dim}=${next.value}` } : {})
    } else {
      navigate(
        next
          ? `/${target}?facet=${encodeURIComponent(`${next.dim}=${next.value}`)}`
          : `/${target}`,
      )
      window.scrollTo({ top: 0, behavior: 'smooth' })
    }
    onClose()
  }

  // 筛选树区：树顶「全部标记」导航节点常在（登录即渲染，聚合加载中
  // 也可导航），三类别根随聚合数据到位渐次出现——某类别失败/未同步
  // 该根不出现
  const facetsData = facetsSnapshot.data
  const facetSection = !uid ? null : (
    <section aria-label="分面筛选" className="site-sidebar__facet">
      <div className="site-sidebar__section-hint">筛选</div>
      <FacetTree
        activeCategory={category}
        expandedTags={expandedTags}
        facetsByCategory={facetsData ?? {}}
        onExpandedTagsChange={setExpandedTags}
        onSelect={handleFacetSelect}
        selection={selection}
      />
    </section>
  )

  return (
    <div className="site-sidebar">
      <SidebarHeader />
      <div className="site-sidebar__body">
        <SidebarStats />
        <nav aria-label="条目导航" className="site-sidebar__nav">
          <TagTree
            aria-label="条目导航"
            nodes={navNodes}
            onSelect={(tag) => {
              if (tag === ALL_MARKS_TAG) handleFacetSelect(ALL_MARKS_TAG, null)
            }}
            selectedTag={category === ALL_MARKS_TAG ? ALL_MARKS_TAG : undefined}
            variant="no-action"
          />
        </nav>
        {facetSection}
      </div>
      <SidebarDebugSwitch />
    </div>
  )
}
