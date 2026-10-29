import { useMemo } from 'react'

import { TagTree } from 'weimo-ui-tagtree/components/tag-tree'
import type { TagTreeNode } from 'weimo-ui-tagtree/components/tag-tree'

import type { DoubanFacets } from './api'
import { DIMENSIONS } from './facet-model'
import type { FacetSelection } from './facet-model'
import type { MarksCategory } from './navigation'

/** 标签值含 / 会破坏 tag 路径的层级语义，替换为全角 */
function escapeTagValue(value: string): string {
  return value.replaceAll('/', '／')
}

type TreeIndex = {
  nodes: TagTreeNode[]
  /** tag 路径 → 原始分面值（反向查选中） */
  byTag: Map<string, FacetSelection>
}

/** 分面聚合 → 标签树索引：维度为根、值为叶。叶 label 拼「值 · 计数」
 *  （tagtree 0.2.x 的 itemCount 类型存在但行组件不渲染）；计数已按
 *  降序返回，直接保序即可 */
function buildTree(category: MarksCategory, facets: DoubanFacets): TreeIndex {
  const byTag = new Map<string, FacetSelection>()
  const nodes = DIMENSIONS[category]
    .map(({ key, label }) => {
      const children = (facets.facets[key] ?? []).map(({ value, count }) => {
        const tag = `${key}/${escapeTagValue(value)}`
        byTag.set(tag, { dim: key, value })
        return { tag, label: `${value} · ${count}` }
      })
      return { tag: key, label, children }
    })
    .filter((node) => node.children.length > 0)
  return { nodes, byTag }
}

/** 分面筛选树（纯展示 + 选中回调）：仅叶子是筛选条件——维度根点击只
 *  管展开（组件内置折叠开关）；再点已选中的叶子取消筛选 */
export function FacetTree({
  category,
  facets,
  selection,
  onSelect,
}: {
  category: MarksCategory
  facets: DoubanFacets
  selection: FacetSelection | null
  onSelect: (selection: FacetSelection | null) => void
}) {
  const { nodes, byTag } = useMemo(
    () => buildTree(category, facets),
    [category, facets],
  )
  const selectedTag = selection
    ? `${selection.dim}/${escapeTagValue(selection.value)}`
    : undefined

  return (
    <TagTree
      aria-label="分面筛选"
      nodes={nodes}
      onSelect={(tag, node) => {
        if (node.children?.length) return
        const mapped = byTag.get(tag)
        if (!mapped) return
        const isSame =
          selection?.dim === mapped.dim && selection.value === mapped.value
        onSelect(isSame ? null : mapped)
      }}
      selectedTag={selectedTag}
      variant="no-action"
    />
  )
}
