import { useMemo } from 'react'
import type { ReactNode } from 'react'
import { BookOpen, Clapperboard, Music } from 'lucide-react'

import { TagTree } from 'weimo-ui-tagtree/components/tag-tree'
import type { TagTreeNode } from 'weimo-ui-tagtree/components/tag-tree'

import type { DoubanFacets } from './api'
import { PRIMARY_DIMENSION, facetValueLabel } from './facet-model'
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

/** 三类别分面聚合 → 二级常驻树：类别（movie/book/music）→ 主维度值叶
 *  （电影=类型、图书=出版社、音乐=流派，见 PRIMARY_DIMENSION）。
 *  叶 label 拼「值 · 计数」（tagtree 0.2.x 的 itemCount 类型存在但行
 *  组件不渲染）；计数已按降序返回直接保序；某类别缺失（未同步/拉取
 *  失败）或主维度无值则不出根。叶 tag 保持三段 类别/维度/值——维度
 *  层不再渲染为树节点，但保留三段让选中叶子的类别根自动展开（祖先
 *  展开按树结构）且不破坏 tag 语义 */
function buildTree(
  facetsByCategory: Partial<Record<MarksCategory, DoubanFacets>>,
): TreeIndex {
  const byTag = new Map<string, FacetLeaf>()
  const nodes = MARKS_CATEGORIES.flatMap((category) => {
    const facets = facetsByCategory[category]
    if (!facets) return []
    const { key } = PRIMARY_DIMENSION[category]
    const children = (facets.facets[key] ?? []).map(({ value, count }) => {
      const tag = `${category}/${key}/${escapeTagValue(value)}`
      byTag.set(tag, { category, dim: key, value })
      // __other__ 保留值显示为「其他类型/其他出版社」（后端长尾合并）
      return {
        tag,
        label: `${facetValueLabel(category, key, value)} · ${count}`,
      }
    })
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

/** 分面筛选树（纯展示 + 选中回调，二级结构）：一级类别根点行=筛选到
 *  该类别（onSelect 上抛 null 即「跳该类别、清筛选」，展开/收起仍走
 *  行内折叠开关）；二级值叶（主维度）是唯一筛选条件——当前类别叶子
 *  再点已选中的取消、其他类别叶子交上层跳路由。展开态受控由上层持有
 *  （选中叶子的类别根 TagTree 会自动展开，无需显式记录） */
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
        // 一级类别根（tag 无 /：叶子带类别前缀）：点行=跳该类别清筛选，
        // 展开/收起仍走行内折叠开关
        if (!tag.includes('/')) {
          if (isMarksCategory(tag)) onSelect(tag, null)
          return
        }
        // 防御：二级树叶子无 children，此分支仅拦非常规结构节点
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
