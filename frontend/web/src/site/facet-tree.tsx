import { useMemo } from 'react'

import { TagTree } from 'weimo-ui-tagtree/components/tag-tree'
import type { TagTreeNode } from 'weimo-ui-tagtree/components/tag-tree'

import type { DoubanFacets } from './api'
import { DIMENSIONS } from './facet-model'
import type { FacetLeaf, FacetSelection } from './facet-model'
import { CATEGORY_NOUNS, MARKS_CATEGORIES } from './navigation'
import type { MarksCategory } from './navigation'

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
    return [{ tag: category, label: CATEGORY_NOUNS[category], children }]
  })
  return { nodes, byTag }
}

/** 分面筛选树（纯展示 + 选中回调）：类别/维度节点点击只管展开（组件
 *  内置折叠开关），仅叶子是筛选条件——当前类别叶子再点已选中的取消；
 *  其他类别叶子交给上层跳路由。展开态受控由上层持有（选中叶子的
 *  祖先链 TagTree 会自动展开，无需显式记录） */
export function FacetTree({
  activeCategory,
  expandedTags,
  facetsByCategory,
  onExpandedTagsChange,
  onSelect,
  selection,
}: {
  /** 当前路由类别；选中叶子必属它（URL 派生）。演示页等无类别时传 undefined */
  activeCategory: MarksCategory | undefined
  facetsByCategory: Partial<Record<MarksCategory, DoubanFacets>>
  selection: FacetSelection | null
  expandedTags: string[]
  onExpandedTagsChange: (tags: string[]) => void
  onSelect: (category: MarksCategory, selection: FacetSelection | null) => void
}) {
  const { nodes, byTag } = useMemo(
    () => buildTree(facetsByCategory),
    [facetsByCategory],
  )
  const selectedTag =
    activeCategory && selection
      ? `${activeCategory}/${selection.dim}/${escapeTagValue(selection.value)}`
      : undefined

  return (
    <TagTree
      aria-label="分面筛选"
      expandedTags={expandedTags}
      nodes={nodes}
      onExpandedTagsChange={onExpandedTagsChange}
      onSelect={(tag, node) => {
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
