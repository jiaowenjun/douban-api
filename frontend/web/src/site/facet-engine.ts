/** 标签树的展示逻辑引擎（纯函数，无副作用）——聚合、长尾 singleton
 *  合并、条件聚合、路径筛选全在前端完成；后端只出维度原始数据
 *  （subject-dims 端点）。所有规则与树交互的口径唯一：树显示多少、
 *  筛选就命中多少。 */

import type { DoubanDimsItem } from './api'
import {
  DIMENSIONS,
  OTHER_FACET_VALUE,
  type FacetStep,
} from './facet-model'
import type { MarksCategory } from './navigation'

/** 维度键 → dims 投影的数组字段名（派生维度 decade/is_tv 走特判） */
const DIM_FIELDS: Record<string, string> = {
  author: 'authors',
  country: 'countries',
  director: 'directors',
  genre: 'genres',
  press: 'press',
  singer: 'singers',
  translator: 'translators',
}

/** 长尾合并的维度开关：仅树上展示且长尾明显的维度开启——影视类型、
 *  图书出版社；音乐流派不合并（值数少，singleton 保留原样更直观） */
function mergeEnabled(category: MarksCategory, dim: string): boolean {
  return (
    (category === 'movie' && dim === 'genre') ||
    (category === 'book' && dim === 'press')
  )
}

/** 条目在某维度的值集合（派生维度含中文语义：decade 由 year、is_tv
 *  由布尔映射电影/电视剧/未分类）；数组保序去重（intro 合成可能重复），
 *  无值返回空集合 */
function itemValues(
  category: MarksCategory,
  dim: string,
  item: DoubanDimsItem,
): string[] {
  if (dim === 'decade') {
    const y = item.dims.year
    return y && y > 1900 && y < 2100 ? [`${Math.floor(y / 10) * 10}s`] : []
  }
  if (dim === 'is_tv') {
    if (category !== 'movie') return []
    const v = item.dims.is_tv
    if (v === undefined) return ['未分类']
    return v ? ['电视剧'] : ['电影']
  }
  const field = DIM_FIELDS[dim]
  const list = field ? item.dims[field as keyof DoubanDimsItem['dims']] : undefined
  if (!Array.isArray(list)) return []
  const out: string[] = []
  for (const v of list) {
    if (v && !out.includes(v)) out.push(v)
  }
  return out
}

/** 某维度在某条目集上的 singleton 值集（计数恰为 1 的值）——「其他」
 *  聚合与过滤匹配的共同依据 */
function singletonSet(
  category: MarksCategory,
  dim: string,
  items: readonly DoubanDimsItem[],
): Set<string> {
  const counts = new Map<string, number>()
  for (const item of items) {
    for (const v of itemValues(category, dim, item)) {
      counts.set(v, (counts.get(v) ?? 0) + 1)
    }
  }
  const out = new Set<string>()
  for (const [value, count] of counts) {
    if (count === 1) out.add(value)
  }
  return out
}

/** 单条目是否命中筛选路径（各步 AND）。__other__ 保留值的 singleton
 *  基集 = 按「全部非 __other__ 步骤」过滤后的条目集（与聚合的条件
 *  口径一致：树在「电影」分支下显示的「其他类型 · N」与点击命中
 *  严格相等） */
export function itemMatches(
  category: MarksCategory,
  item: DoubanDimsItem,
  steps: readonly FacetStep[],
  allItems: readonly DoubanDimsItem[],
): boolean {
  if (steps.length === 0) return true
  const plain = steps.filter((s) => s.value !== OTHER_FACET_VALUE)
  const scoped = plain.length
    ? allItems.filter((it) =>
        plain.every((s) => itemValues(category, s.dim, it).includes(s.value)),
      )
    : allItems
  return steps.every((step) => {
    const have = itemValues(category, step.dim, item)
    if (step.value !== OTHER_FACET_VALUE) return have.includes(step.value)
    const singletons = singletonSet(category, step.dim, scoped)
    return have.some((v) => singletons.has(v))
  })
}

export type DimCounts = { value: string; count: number }[]

/** 聚合：各维度值计数（计数 = 含该值的条目数），排序计数降序、同计数
 *  按值升序；开启维度的 singleton 值 ≥2 个时收进 __other__ 钉末尾
 *  （仅 1 个保留原值），其计数 = 含 singleton 值的条目数（多 singleton
 *  值的条目只计一次）。filters 提供时先按命中集过滤再聚合（条件聚合，
 *  如影视形态层分支内的类型分布） */
export function aggregateDims(
  category: MarksCategory,
  items: readonly DoubanDimsItem[],
  filters: readonly FacetStep[] = [],
): Record<string, DimCounts> {
  const scoped = filters.length
    ? items.filter((it) => itemMatches(category, it, filters, items))
    : items
  const out: Record<string, DimCounts> = {}
  for (const { key: dim } of DIMENSIONS[category]) {
    const counts = new Map<string, number>()
    for (const item of scoped) {
      for (const v of itemValues(category, dim, item)) {
        counts.set(v, (counts.get(v) ?? 0) + 1)
      }
    }
    const list: DimCounts = [...counts.entries()].map(([value, count]) => ({
      value,
      count,
    }))
    list.sort((a, b) => b.count - a.count || (a.value < b.value ? -1 : 1))
    if (mergeEnabled(category, dim)) {
      const singletons = new Set(
        list.filter((x) => x.count === 1).map((x) => x.value),
      )
      if (singletons.size >= 2) {
        const otherCount = scoped.filter((it) =>
          itemValues(category, dim, it).some((v) => singletons.has(v)),
        ).length
        out[dim] = [
          ...list.filter((x) => x.count > 1),
          { value: OTHER_FACET_VALUE, count: otherCount },
        ]
        continue
      }
    }
    out[dim] = list
  }
  return out
}

/** 路径 → 命中条目 id（保持 items 的 date DESC 稳定序——分页切片的
 *  顺序权威） */
export function filterIds(
  category: MarksCategory,
  items: readonly DoubanDimsItem[],
  path: readonly FacetStep[],
): string[] {
  if (path.length === 0) return items.map((it) => it.id)
  return items
    .filter((it) => itemMatches(category, it, path, items))
    .map((it) => it.id)
}
