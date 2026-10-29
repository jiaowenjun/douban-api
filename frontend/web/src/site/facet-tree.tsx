import { useMemo } from 'react'
import type { ReactNode } from 'react'
import { BookOpen, Clapperboard, Music } from 'lucide-react'

import { TagTree } from 'weimo-ui-tagtree/components/tag-tree'
import type { TagTreeNode } from 'weimo-ui-tagtree/components/tag-tree'

import type { DoubanFacets } from './api'
import { DIMENSIONS } from './facet-model'
import type { FacetLeaf, FacetSelection } from './facet-model'
import {
  ALL_MARKS_TAG,
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

/** 标签值含 / 会破坏 tag 路径的层级语义，替换为全角 */
function escapeTagValue(value: string): string {
  return value.replaceAll('/', '／')
}

type TreeIndex = {
  nodes: TagTreeNode[]
  /** tag 路径 → 叶子分面条件（反向查选中/点击） */
  byTag: Map<string, FacetLeaf>
}

/** 三类别分面聚合 → 三级常驻树：类别（movie/book/music）→ 维度 → 值叶。
 *  维度 label 拼「值 · 计数」（tagtree 0.2.x 的 itemCount 类型存在但行
 *  组件不渲染）；计数已按降序返回直接保序；某类别缺失（未同步/拉取
 *  失败）或全维度为空则不出根 */
function buildTree(
  facetsByCategory: Partial<Record<MarksCategory, DoubanFacets>>,
): TreeIndex {
  const byTag = new Map<string, FacetLeaf>()
  const nodes = MARKS_CATEGORIES.flatMap((category) => {
    const facets = facetsByCategory[category]
    if (!facets) return []
    const children = DIMENSIONS[category]
      .map(({ key, label }) => {
        const valueNodes = (facets.facets[key] ?? []).map(({ value, count }) => {
          const tag = `${category}/${key}/${escapeTagValue(value)}`
          byTag.set(tag, { category, dim: key, value })
          return { tag, label: `${value} · ${count}` }
        })
        return { tag: `${category}/${key}`, label, children: valueNodes }
      })
      .filter((node) => node.children.length > 0)
    if (children.length === 0) return []
    return [{
      tag: category,
      label: CATEGORY_NOUNS[category],
      icon: CATEGORY_ICONS[category],
      children,
    }]
  })
  return { nodes, byTag }
}

/** 分面筛选树（纯展示 + 选中回调）：一级类别根点行=筛选到该类别
 *  （onSelect 上抛 null 即「跳该类别、清筛选」，展开/收起仍走行内
 *  折叠开关）；维度节点点击只管展开（组件内置折叠开关），仅叶子是
 *  筛选条件——当前类别叶子再点已选中的取消、其他类别叶子交上层跳
 *  路由。展开态受控由上层持有（选中叶子的祖先链 TagTree 会自动展开，
 *  无需显式记录） */
export function FacetTree({
  activeCategory,
  expandedTags,
  facetsByCategory,
  onExpandedTagsChange,
  onSelect,
  selection,
}: {
  /** 当前路由目标：真实类别或 all（合并视图，无分面）；选中指示必属它
   *  （URL 派生）。演示页等无类别时传 undefined */
  activeCategory: NavTarget | undefined
  facetsByCategory: Partial<Record<MarksCategory, DoubanFacets>>
  selection: FacetSelection | null
  expandedTags: string[]
  onExpandedTagsChange: (tags: string[]) => void
  onSelect: (target: NavTarget, selection: FacetSelection | null) => void
}) {
  const { nodes, byTag } = useMemo(
    () => buildTree(facetsByCategory),
    [facetsByCategory],
  )
  // 选中指示两档：有筛选选中叶子；无筛选时当前导航目标顶上（类别根
  // 或「全部标记」节点——原导航树的当前位置指示由根接管）
  const selectedTag =
    selection && activeCategory && activeCategory !== ALL_MARKS_TAG
      ? `${activeCategory}/${selection.dim}/${escapeTagValue(selection.value)}`
      : activeCategory

  return (
    <TagTree
      aria-label="分面筛选"
      expandedTags={expandedTags}
      nodes={nodes}
      onExpandedTagsChange={onExpandedTagsChange}
      onSelect={(tag, node) => {
        // 一级类别根（tag 无 /：维度/叶子都带类别前缀）：点行=跳该类别
        // 清筛选，展开/收起仍走行内折叠开关
        if (!tag.includes('/')) {
          if (isMarksCategory(tag)) onSelect(tag, null)
          return
        }
        if (node.children?.length) return
        const leaf = byTag.get(tag)
        if (!leaf) return
        const isSame =
          activeCategory === leaf.category &&
          selection?.dim === leaf.dim &&
          selection.value === leaf.value
        onSelect(
          leaf.category,
          isSame ? null : { dim: leaf.dim, value: leaf.value },
        )
      }}
      selectedTag={selectedTag}
      variant="no-action"
    />
  )
}
