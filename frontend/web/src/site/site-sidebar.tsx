import { useState } from 'react'
import { Grid2X2 } from 'lucide-react'
import { useLocation, useNavigate, useSearchParams } from 'react-router'
import { TagTree, type TagTreeNode } from 'weimo-ui-tagtree/components/tag-tree'

import type { DoubanDims } from './api'
import { useDoubanDimsQuery } from './douban-queries'
import { useDoubanUser } from './douban-user'
import { FacetTree } from './facet-tree'
import {
  facetPathKey,
  parseFacetPath,
  pathTag,
  type FacetStep,
} from './facet-model'
import { SidebarDebugSwitch } from './sidebar-debug-switch'
import { SidebarHeader } from './sidebar-header'
import { SidebarStats } from './sidebar-stats'
import {
  ALL_MARKS_TAG,
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
 *  （StatGroup/Heatmap）→ 导航区（全部标记）→ 分面筛选树（影视各
 *  形态提升一级、图书/音乐为分组值叶→主维度值叶三层；类别根/形态
 *  节点带图标，图书/音乐根点行=跳该类别清筛选兼任导航，影视形态
 *  节点点行=设形态筛选）。
 *  筛选路径经 URL ?facet=dim=value;… 与列表页共享（侧边栏写、列表页
 *  读，影视叶为 is_tv+genre 双条件）；类别切换天然清除筛选。
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
  const path = realCategory
    ? parseFacetPath(realCategory, searchParams.get('facet'))
    : []

  // 条目维度数据：三类别并行查询（轻量投影，聚合/长尾合并/条件计数
  // 全部由前端 facet-engine 本地算），与列表页共用同一缓存（key 带
  // uid 作用域，10 分钟新鲜期内类别来回切零请求）；单类别失败/未同步
  // 该根不出现
  const movieDims = useDoubanDimsQuery(uid, 'movie')
  const bookDims = useDoubanDimsQuery(uid, 'book')
  const musicDims = useDoubanDimsQuery(uid, 'music')
  const dimsData: Partial<Record<MarksCategory, DoubanDims>> = {
    movie: movieDims.data ?? undefined,
    book: bookDims.data ?? undefined,
    music: musicDims.data ?? undefined,
  }

  // 展开态：类别切换时重置为「当前类别根展开」；用户手动展开的
  // 其他类别在同一类别会话内保留。选中叶子到维度的链路不必记录
  // ——TagTree 对选中祖先自动展开
  const [expandedTags, setExpandedTags] = useState<string[]>([])
  const [expandedCategory, setExpandedCategory] = useState<
    NavTarget | undefined | null
  >(null)
  if (category !== expandedCategory) {
    setExpandedCategory(category)
    // 类别切换重置展开态：图书/音乐展开类别根；影视无类别根（形态
    // 节点一级），带形态筛选进入时展开首步形态节点——选中叶的祖先
    // 自动展开覆盖不到形态根自身，无筛选则收起（形态行一级可见，
    // 点击筛选时自行展开）；all 亦无根可展
    setExpandedTags(
      category === 'movie'
        ? path.length > 0
          ? [pathTag('movie', path.slice(0, 1))]
          : []
        : category !== undefined && category !== ALL_MARKS_TAG
          ? [category]
          : [],
    )
  }

  /** 筛选路径统一处理：目标=当前类别时改查询参数即生效（空路径即清除
   *  筛选——再点取消与一级根点击重置都走这里）；目标为其他类别或
   *  「全部标记」跳路由，带路径一步落位、导航节点跳纯类别路由 */
  function handlePathSelect(target: NavTarget, next: FacetStep[]) {
    if (target === category) {
      setSearchParams(next.length ? { facet: facetPathKey(next) } : {})
    } else {
      navigate(
        next.length
          ? `/${target}?facet=${encodeURIComponent(facetPathKey(next))}`
          : `/${target}`,
      )
      window.scrollTo({ top: 0, behavior: 'smooth' })
    }
    onClose()
  }

  // 筛选树区：树顶「全部标记」导航节点常在（登录即渲染，聚合加载中
  // 也可导航），三类别根随聚合数据到位渐次出现——某类别失败/未同步
  // 该根不出现
  const facetSection = !uid ? null : (
    <section aria-label="分面筛选" className="site-sidebar__facet">
      <div className="site-sidebar__section-hint">筛选</div>
      <FacetTree
        activeCategory={category}
        dimsByCategory={dimsData}
        expandedTags={expandedTags}
        onExpandedTagsChange={setExpandedTags}
        onPathSelect={handlePathSelect}
        path={path}
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
              if (tag === ALL_MARKS_TAG) handlePathSelect(ALL_MARKS_TAG, [])
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
