import { useMemo } from 'react'
import type { ReactNode } from 'react'
import { BookOpen, Clapperboard, Film, Music, Tv } from 'lucide-react'

import { TagTree } from 'weimo-ui-tagtree/components/tag-tree'
import type { TagTreeNode } from 'weimo-ui-tagtree/components/tag-tree'

import type { DoubanDims } from './api'
import { aggregateDims } from './facet-engine'
import {
  KIND_DIMENSION,
  PRIMARY_DIMENSION,
  escapeFacetValue,
  facetValueLabel,
  pathTag,
} from './facet-model'
import type { FacetStep } from './facet-model'
import {
  CATEGORY_NOUNS,
  isMarksCategory,
  MARKS_CATEGORIES,
  type NavTarget,
} from './navigation'
import type { MarksCategory } from './navigation'

/** 一级类别根行图标（根兼任类别导航，观感对齐原导航树行）；movie 项
 *  兼作影视形态一级节点的兜底图标（无专属图标的形态值用） */
const CATEGORY_ICONS: Record<MarksCategory, ReactNode> = {
  book: <BookOpen aria-hidden="true" />,
  movie: <Clapperboard aria-hidden="true" />,
  music: <Music aria-hidden="true" />,
}

/** 影视形态一级节点图标：电影/电视剧专属，其余形态值（如未分类）
 *  回退 Clapperboard */
const KIND_ICONS: Record<string, ReactNode> = {
  电影: <Film aria-hidden="true" />,
  电视剧: <Tv aria-hidden="true" />,
}

type TagAction = {
  category: MarksCategory
  /** 点击该节点应设置的筛选路径（空路径 = 清除） */
  path: FacetStep[]
}

type TreeIndex = {
  nodes: TagTreeNode[]
  /** tag 路径 → 点击动作 */
  byTag: Map<string, TagAction>
}

/** 影视形态一级节点（两层）：各形态（is_tv 派生：电影/电视剧/未分类，
 *  计数取全局聚合）直接作一级节点——无「影视」类别根（原挂其下，
 *  提升后少一层钻取），图标对齐类别根观感——其下挂主维度值叶
 *  （条件聚合，__other__ 显示「其他类型」）。形态行点击=单步筛选，
 *  值叶点击=形态+主维度双条件 */
function buildMovieNodes(
  items: DoubanDims['items'],
  byTag: Map<string, TagAction>,
): TagTreeNode[] {
  const kindKey = KIND_DIMENSION?.key
  if (!kindKey) return []
  const rootCounts = aggregateDims('movie', items)
  return (rootCounts[kindKey] ?? []).map(({ value, count }) => {
    const kindTag = `movie/${kindKey}/${escapeFacetValue(value)}`
    byTag.set(kindTag, { category: 'movie', path: [{ dim: kindKey, value }] })
    // 形态分支内的类型分布：条件聚合（本地纯函数）
    const scoped = aggregateDims('movie', items, [{ dim: kindKey, value }])
    const { key: leafDim } = PRIMARY_DIMENSION.movie
    const children = (scoped[leafDim] ?? []).map(
      ({ value: leafValue, count: leafCount }) => {
        const tag = `${kindTag}/${leafDim}/${escapeFacetValue(leafValue)}`
        byTag.set(tag, {
          category: 'movie',
          path: [
            { dim: kindKey, value },
            { dim: leafDim, value: leafValue },
          ],
        })
        return {
          tag,
          label: facetValueLabel('movie', leafDim, leafValue),
          itemCount: leafCount,
        }
      },
    )
    return {
      tag: kindTag,
      label: value,
      itemCount: count,
      icon: KIND_ICONS[value] ?? CATEGORY_ICONS.movie,
      children,
    }
  })
}

/** 图书/音乐分支（两层）：类别根 → 主维度值叶（全局聚合） */
function buildSimpleChildren(
  category: MarksCategory,
  items: DoubanDims['items'],
  byTag: Map<string, TagAction>,
): TagTreeNode[] {
  const { key } = PRIMARY_DIMENSION[category]
  const counts = aggregateDims(category, items)
  return (counts[key] ?? []).map(({ value, count }) => {
    const tag = `${category}/${key}/${escapeFacetValue(value)}`
    byTag.set(tag, { category, path: [{ dim: key, value }] })
    return {
      tag,
      label: facetValueLabel(category, key, value),
      itemCount: count,
    }
  })
}

/** 三类别常驻树：影视无类别根——各形态节点（电影/电视剧/…，见
 *  buildMovieNodes）直接一级、与图书/音乐的类别根并列；图书/音乐为
 *  「类别根（点行=跳该类别清筛选）→ 主维度值叶」两层。叶/形态行计数
 *  走 itemCount（tagtree 0.3.0 起渲染在行尾折叠开关左侧，与开关同占位
 *  色）；计数已按降序返回直接保序；某类别缺失（未同步/拉取失败）
 *  或无值则不出根。叶 tag 保持「类别/维度(+/维度)/值」多段——中间
 *  维度不出现在行 label 但保留在 tag 语义中（选中叶的祖先自动展开
 *  按树结构生效） */
function buildTree(
  dimsByCategory: Partial<Record<MarksCategory, DoubanDims>>,
): TreeIndex {
  const byTag = new Map<string, TagAction>()
  const nodes = MARKS_CATEGORIES.flatMap((category) => {
    const dims = dimsByCategory[category]
    if (!dims) return []
    // 影视：形态节点多个直接作一级；图书/音乐：单类别根包主维度叶
    if (category === 'movie' && KIND_DIMENSION) {
      return buildMovieNodes(dims.items, byTag)
    }
    const children = buildSimpleChildren(category, dims.items, byTag)
    if (children.length === 0) return []
    return [
      {
        tag: category,
        label: CATEGORY_NOUNS[category],
        icon: CATEGORY_ICONS[category],
        children,
      },
    ]
  })
  return { nodes, byTag }
}

/** 分面筛选树（纯展示 + 路径回调）：图书/音乐的一级类别根点行=跳该
 *  类别清筛选（onPathSelect 上抛空路径，展开/收起仍走行内折叠开关）；
 *  影视形态节点（一级）与各值叶是筛选条件——点击设路径（已选同路径
 *  再点取消），跨类别值叶交上层跳路由。展开态受控由上层持有（选中
 *  叶的祖先链 TagTree 会自动展开） */
export function FacetTree({
  activeCategory,
  dimsByCategory,
  expandedTags,
  onExpandedTagsChange,
  onPathSelect,
  path,
}: {
  /** 当前路由目标：真实类别或 all（合并视图，无分面）；选中指示必属
   *  它（URL 派生）。演示页等无类别时传 undefined */
  activeCategory: NavTarget | undefined
  /** 三类别维度数据（聚合/条件计数由 facet-engine 本地算） */
  dimsByCategory: Partial<Record<MarksCategory, DoubanDims>>
  /** 当前类别的筛选路径（URL 派生，顺序即步骤） */
  path: FacetStep[]
  expandedTags: string[]
  onExpandedTagsChange: (tags: string[]) => void
  /** 路径改写：目标类别 + 完整路径（空数组 = 清除筛选） */
  onPathSelect: (target: NavTarget, path: FacetStep[]) => void
}) {
  const { nodes, byTag } = useMemo(
    () => buildTree(dimsByCategory),
    [dimsByCategory],
  )
  // 选中指示两档：筛选中选中末步对应节点；无筛选时当前导航目标顶上
  // （类别根或「全部标记」——原导航树的当前位置指示由根接管）
  const selectedTag =
    activeCategory !== undefined && path.length > 0
      ? pathTag(activeCategory, path)
      : activeCategory

  return (
    <TagTree
      aria-label="分面筛选"
      expandedTags={expandedTags}
      nodes={nodes}
      onExpandedTagsChange={onExpandedTagsChange}
      onSelect={(tag) => {
        // 一级类别根（图书/音乐，tag 无 /）：点行=跳该类别清筛选
        if (!tag.includes('/')) {
          if (isMarksCategory(tag)) onPathSelect(tag, [])
          return
        }
        const hit = byTag.get(tag)
        if (!hit) return
        // 再点已选路径 = 取消；跨类别/其他路径 = 设置
        const isSame =
          activeCategory === hit.category &&
          hit.path.length === path.length &&
          hit.path.every(
            (step, i) => path[i].dim === step.dim && path[i].value === step.value,
          )
        onPathSelect(hit.category, isSame ? [] : hit.path)
      }}
      selectedTag={selectedTag}
      variant="no-action"
    />
  )
}
