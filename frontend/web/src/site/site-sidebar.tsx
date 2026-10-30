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
  MARKS_CATEGORIES,
  NAV_RATING_STEPS,
  parseRatingParam,
  ratingKeyMatches,
  ratingNavTag,
  resolveMarksView,
  type MarksCategory,
  type NavTarget,
  type RatingKey,
} from './navigation'
import './site-sidebar.css'

/** 站点侧边栏：整体接管模板 sidebar 插槽，布局对齐 weimo-biji 的
 *  biji-side-bar——身份栏固定在顶，滚动主体依次为标记统计区
 *  （StatGroup/Heatmap）→ 导航区（全部标记，下挂我的评分子节点）→
 *  分面筛选树（影视各形态提升一级、图书/音乐为分组值叶→主维度值叶
 *  三层；类别根/形态节点带图标，图书/音乐根点行=跳该类别清筛选兼任
 *  导航，影视形态节点点行=设形态筛选）。
 *  筛选路径经 URL ?facet=dim=value;… 与列表页共享（侧边栏写、列表页
 *  读，影视叶为 is_tv+genre 双条件）；评分筛选走 ?rating=N（仅全部
 *  标记视图）；类别切换天然清除筛选。
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
  // 我的评分筛选（仅 all 合并视图有此参数；非法值按无筛选处理）
  const rating =
    category === ALL_MARKS_TAG
      ? parseRatingParam(searchParams.get('rating'))
      : undefined

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

  // 导航区节点：全部标记（书影音合并视图入口，图标对齐 biji-side-bar
  // 全部笔记的 Grid2X2；类别导航由筛选树一级根兼任，不在此重复），其
  // 下挂我的评分快捷筛选子节点（点击设 ?rating=，见 handleRatingSelect）。
  // 行尾计数随 dims 数据挂：全部标记=三类别条目总数（dims 全量投影
  // length 求和，与筛选树类别根/统计区同源——某类别未同步/拉取失败
  // 数据为 null 计 0，合并视图本就只含已同步类别）；评分子节点=
  // 键命中数（ratingKeyMatches，'lteN' 区间与后端 <= 比较同口径，
  // my_rating 随 dims item 平铺下发）。三类别查询全部 settle 才显示
  // （失败也 settle 为 null 数据），避免加载中途先亮部分和再跳变
  const dimsQueries = { movie: movieDims, book: bookDims, music: musicDims }
  const countsReady =
    uid !== null && MARKS_CATEGORIES.every((c) => !dimsQueries[c].isPending)
  const navItems = MARKS_CATEGORIES.flatMap(
    (c) => dimsData[c]?.items ?? [],
  )
  const ratingCount = (key: RatingKey) =>
    navItems.filter((item) => ratingKeyMatches(key, item.my_rating)).length
  const navNodes: TagTreeNode[] = [
    {
      tag: ALL_MARKS_TAG,
      label: '全部标记',
      icon: <Grid2X2 aria-hidden="true" />,
      itemCount: countsReady ? navItems.length : undefined,
      children: NAV_RATING_STEPS.map(({ key, label }) => ({
        tag: ratingNavTag(key),
        label,
        itemCount: countsReady ? ratingCount(key) : undefined,
      })),
    },
  ]

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

  /** 「全部标记」评分子节点：目标必为 all——已在 all 视图改 ?rating=
   *  即生效（再点同评分取消，父行点击经 handlePathSelect 空路径全清），
   *  在其他类别则跳路由携带 */
  function handleRatingSelect(key: RatingKey) {
    if (category === ALL_MARKS_TAG) {
      setSearchParams(rating === key ? {} : { rating: key })
    } else {
      navigate(`/${ALL_MARKS_TAG}?rating=${key}`)
      window.scrollTo({ top: 0, behavior: 'smooth' })
    }
    onClose()
  }

  // 筛选树区：树顶「全部标记」导航节点常在（登录即渲染，聚合加载中
  // 也可导航），三类别根随聚合数据到位渐次出现——某类别失败/未同步
  // 该根不出现
  const facetSection = !uid ? null : (
    <section aria-label="分面筛选" className="site-sidebar__facet">
      <div className="site-sidebar__section-hint">分类筛选</div>
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
              if (tag === ALL_MARKS_TAG) {
                handlePathSelect(ALL_MARKS_TAG, [])
                return
              }
              const hit = NAV_RATING_STEPS.find(
                (step) => ratingNavTag(step.key) === tag,
              )
              if (hit) handleRatingSelect(hit.key)
            }}
            // 选中指示两档：all 视图评分筛选中选中对应子节点（祖先
            // 自动展开），否则当前导航目标顶上
            selectedTag={
              category === ALL_MARKS_TAG
                ? rating
                  ? ratingNavTag(rating)
                  : ALL_MARKS_TAG
                : undefined
            }
            variant="no-action"
          />
        </nav>
        {facetSection}
      </div>
      <SidebarDebugSwitch />
    </div>
  )
}
