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

/** 维度的树上展示值上限（DIMENSIONS 配置 topK；未配置即不封顶） */
function topKOf(category: MarksCategory, dim: string): number | undefined {
  return DIMENSIONS[category].find((d) => d.key === dim)?.topK
}

/** movie/genre 在 country 步之下（电影形态国家值叶的三级类型层）的
 *  封顶值：五个最多的类型 + 其他类型 */
const GENRE_TOP_K_UNDER_COUNTRY = 5

/** 维度在给定步骤序列下的树上展示值上限：country 恒为 DIMENSIONS
 *  配置的 topK（国家值叶前五）；movie/genre 仅当序列含 country 步时
 *  封顶（GENRE_TOP_K_UNDER_COUNTRY），其余场合（形态直挂两层的
 *  类型叶）不封顶、走 singleton 合并——展示聚合（传 filters）与
 *  __other__ 匹配（传该步前缀）两侧同源此判定，口径唯一 */
function topKUnder(
  category: MarksCategory,
  dim: string,
  steps: readonly Pick<FacetStep, 'dim'>[],
): number | undefined {
  if (
    category === 'movie' &&
    dim === 'genre' &&
    steps.some((s) => s.dim === 'country')
  ) {
    return GENRE_TOP_K_UNDER_COUNTRY
  }
  return topKOf(category, dim)
}

/** 某维度在某条目集上的值计数（聚合与 __other__ 匹配共用的底座） */
function valueCounts(
  category: MarksCategory,
  dim: string,
  items: readonly DoubanDimsItem[],
): Map<string, number> {
  const counts = new Map<string, number>()
  for (const item of items) {
    for (const v of itemValues(category, dim, item)) {
      counts.set(v, (counts.get(v) ?? 0) + 1)
    }
  }
  return counts
}

/** 某维度在某条目集上的 singleton 值集（计数恰为 1 的值）——「其他」
 *  聚合与过滤匹配的共同依据 */
function singletonSet(
  category: MarksCategory,
  dim: string,
  items: readonly DoubanDimsItem[],
): Set<string> {
  const counts = valueCounts(category, dim, items)
  const out = new Set<string>()
  for (const [value, count] of counts) {
    if (count === 1) out.add(value)
  }
  return out
}

/** __other__ 步骤的命中规则（与聚合折叠同源，树显示多少筛选就命中
 *  多少）：topK 维度=含「展示集外」任意值——展示集按传入的（该步
 *  前缀）命中集内计数降序（同计数按值升序）取前 topK 现算，保证与
 *  树上看到的展示集一致；其余维度（singleton 折叠）=含 singleton 值。
 *  topK 由调用方按 topKUnder(该步前缀) 判定传入 */
function matchesOtherBucket(
  category: MarksCategory,
  dim: string,
  values: readonly string[],
  scoped: readonly DoubanDimsItem[],
  topK: number | undefined,
): boolean {
  if (topK !== undefined) {
    const ranked = [...valueCounts(category, dim, scoped).entries()].sort(
      (a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1),
    )
    const displayed = new Set(ranked.slice(0, topK).map(([v]) => v))
    return values.some((v) => !displayed.has(v))
  }
  const singletons = singletonSet(category, dim, scoped)
  return values.some((v) => singletons.has(v))
}

/** 单条目是否命中筛选路径（各步 AND，逐步累积）。__other__ 步的桶
 *  展示集按「该步之前的前缀步骤命中集」计算——与树上该层的条件聚合
 *  口径一致（桶成员资格在节点展示时已定，不随其后步骤重算，否则
 *  「其他地区→剧情」的树上计数与点击命中会因前五集合在前缀子集内
 *  重排而漂移）。普通步直接包含判定 */
export function itemMatches(
  category: MarksCategory,
  item: DoubanDimsItem,
  steps: readonly FacetStep[],
  allItems: readonly DoubanDimsItem[],
): boolean {
  if (steps.length === 0) return true
  let scope = allItems
  for (let i = 0; i < steps.length; i++) {
    const step = steps[i]
    const stepTopK = topKUnder(category, step.dim, steps.slice(0, i))
    const hit =
      step.value !== OTHER_FACET_VALUE
        ? itemValues(category, step.dim, item).includes(step.value)
        : matchesOtherBucket(
            category,
            step.dim,
            itemValues(category, step.dim, item),
            scope,
            stepTopK,
          )
    if (!hit) return false
    scope = scope.filter((it) =>
      step.value !== OTHER_FACET_VALUE
        ? itemValues(category, step.dim, it).includes(step.value)
        : matchesOtherBucket(
            category,
            step.dim,
            itemValues(category, step.dim, it),
            scope,
            stepTopK,
          ),
    )
  }
  return true
}

export type DimCounts = { value: string; count: number }[]

/** 聚合：各维度值计数（计数 = 含该值的条目数），排序计数降序、同计数
 *  按值升序。展示封顶（topKUnder 按 filters 判定）优先：值数超上限时
 *  仅留计数前 topK（country 国家值叶；genre 在 country 条件聚合下），
 *  其余收 __other__（计数 = 含展示集外值的条目数，多值条目只计一次
 *  ——封顶已含 singleton 长尾，取代 singleton 合并）；未封顶的开启
 *  维度 singleton 值 ≥2 个时收进 __other__ 钉末尾（仅 1 个保留原值）。
 *  filters 提供时先按命中集过滤再聚合（条件聚合，如影视形态层分支内
 *  的类型分布） */
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
    const list: DimCounts = [...valueCounts(category, dim, scoped)].map(
      ([value, count]) => ({ value, count }),
    )
    list.sort((a, b) => b.count - a.count || (a.value < b.value ? -1 : 1))
    const topK = topKUnder(category, dim, filters)
    if (topK !== undefined && list.length > topK) {
      const displayed = new Set(list.slice(0, topK).map((x) => x.value))
      const restCount = scoped.filter((it) =>
        itemValues(category, dim, it).some((v) => !displayed.has(v)),
      ).length
      out[dim] = [
        ...list.slice(0, topK),
        { value: OTHER_FACET_VALUE, count: restCount },
      ]
      continue
    }
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
