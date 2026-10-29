import { useMemo } from 'react'
import type { ReactNode } from 'react'
import { BookOpen, Clapperboard, Music } from 'lucide-react'

import { TagTree } from 'weimo-ui-tagtree/components/tag-tree'
import type { TagTreeNode } from 'weimo-ui-tagtree/components/tag-tree'

import type { DoubanDims } from './api'
import { aggregateDims } from './facet-engine'
import {
  KIND_DIMENSION,
  PRIMARY_DIMENSION,
  escapeFacetValue,
  facetValueLabel,
} from './facet-model'
import type { FacetStep } from './facet-model'
import {
  CATEGORY_NOUNS,
  isMarksCategory,
  MARKS_CATEGORIES,
  type NavTarget,
} from './navigation'
import type { MarksCategory } from './navigation'

/** 一级类别根行图标（根兼任类别导航，观感对齐原导航树行） */
const CATEGORY_ICONS: Record<MarksCategory, ReactNode> = {
  book: <BookOpen aria-hidden="true" />,
  movie: <Clapperboard aria-hidden="true" />,
  music: <Music aria-hidden="true" />,
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

/** 影视分支（三层）：类别根 → 形态（is_tv 派生：电影/电视剧/未分类，
 *  计数取全局聚合）→ 主维度值叶（条件聚合，__other__ 显示「其他类型」）。
 *  形态行点击=单步筛选，值叶点击=形态+主维度双条件 */
function buildMovieChildren(
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
          label: `${facetValueLabel('movie', leafDim, leafValue)} · ${leafCount}`,
        }
      },
    )
    return { tag: kindTag, label: `${value} · ${count}`, children }
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
      label: `${facetValueLabel(category, key, value)} · ${count}`,
    }
  })
}

/** 三类别常驻树：类别根（点行=跳该类别清筛选）→ 影视为 形态→类型
 *  三层、图书/音乐为主维度单层。叶/形态行 label 拼「值 · 计数」
 *  （tagtree 0.2.x 的 itemCount 类型存在但行组件不渲染）；计数已按
 *  降序返回直接保序；某类别缺失（未同步/拉取失败）或无值则不出根。
 *  叶 tag 保持「类别/维度(+/维度)/值」多段——中间维度不出现在行
 *  label 但保留在 tag 语义中（选中叶的祖先自动展开按树结构生效） */
function buildTree(
  dimsByCategory: Partial<Record<MarksCategory, DoubanDims>>,
): TreeIndex {
  const byTag = new Map<string, TagAction>()
  const nodes = MARKS_CATEGORIES.flatMap((category) => {
    const dims = dimsByCategory[category]
    if (!dims) return []
    const children =
      category === 'movie' && KIND_DIMENSION
        ? buildMovieChildren(dims.items, byTag)
        : buildSimpleChildren(category, dims.items, byTag)
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

/** 分面筛选树（纯展示 + 路径回调）：一级类别根点行=跳该类别清筛选
 *  （onPathSelect 上抛空路径，展开/收起仍走行内折叠开关）；影视的
 *  形态行与各值叶是筛选条件——点击设路径（已选同路径再点取消），
 *  跨类别值叶交上层跳路由。展开态受控由上层持有（选中叶的祖先链
 *  TagTree 会自动展开） */
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
        // 一级类别根（tag 无 /）：点行=跳该类别清筛选
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

/** 筛选路径 → 树节点 tag（与 buildTree 的拼装规则一致，选中指示用） */
function pathTag(category: NavTarget, path: readonly FacetStep[]): string {
  const [first, ...rest] = path
  let tag = `${category}/${first.dim}/${escapeFacetValue(first.value)}`
  for (const step of rest) {
    tag += `/${step.dim}/${escapeFacetValue(step.value)}`
  }
  return tag
}
