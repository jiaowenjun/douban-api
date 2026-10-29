import { useEffect, useState } from 'react'
import { useLocation, useNavigate, useSearchParams } from 'react-router'

import { fetchDoubanFacets, type DoubanFacets } from './api'
import { useDoubanUser } from './douban-user'
import { FacetTree } from './facet-tree'
import { parseFacetParam, type FacetSelection } from './facet-model'
import { SidebarDebugSwitch } from './sidebar-debug-switch'
import { SidebarHeader } from './sidebar-header'
import { SidebarStats } from './sidebar-stats'
import {
  MARKS_CATEGORIES,
  resolveMarksView,
  type MarksCategory,
} from './navigation'
import './site-sidebar.css'

/** 站点侧边栏：整体接管模板 sidebar 插槽，布局对齐 weimo-biji 的
 *  biji-side-bar——身份栏固定在顶，滚动主体依次为标记统计区
 *  （StatGroup/Heatmap）→ 分面筛选树。树的一级类别根（电影/图书/
 *  音乐，带图标）兼任类别导航：点行=跳该类别并清筛选，原独立导航
 *  树已因其可替代而移除。分面选择经 URL ?facet=dim=value 与列表页
 *  共享（侧边栏写、列表页读）；类别切换天然清除筛选。
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
    MarksCategory | undefined | null
  >(null)
  if (category !== expandedCategory) {
    setExpandedCategory(category)
    setExpandedTags(category ? [category] : [])
  }

  /** 分面选择统一处理：目标=当前类别时改查询参数即生效（null 即清除
   *  筛选——叶子再点取消与一级类别根点击重置都走这里）；目标为其他
   *  类别（叶子或根）跳路由，带值叶子一步带筛选、根跳纯类别路由 */
  function handleFacetSelect(
    target: MarksCategory,
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

  // 筛选树区：未登录或三类别聚合全空（全失败/全未同步）不渲染；
  // 加载中给占位行
  const facetsData = facetsSnapshot.data
  const hasFacets = !!facetsData && Object.keys(facetsData).length > 0
  const facetSection =
    !uid || (!hasFacets && !facetsSnapshot.loading) ? null : (
      <section aria-label="分面筛选" className="site-sidebar__facet">
        <div className="site-sidebar__section-hint">筛选</div>
        {hasFacets ? (
          <FacetTree
            activeCategory={category}
            expandedTags={expandedTags}
            facetsByCategory={facetsData}
            onExpandedTagsChange={setExpandedTags}
            onSelect={handleFacetSelect}
            selection={selection}
          />
        ) : (
          <div className="site-sidebar__status" role="status">
            筛选加载中…
          </div>
        )}
      </section>
    )

  return (
    <div className="site-sidebar">
      <SidebarHeader />
      <div className="site-sidebar__body">
        <SidebarStats />
        {facetSection}
      </div>
      <SidebarDebugSwitch />
    </div>
  )
}
