import type { MarksCategory } from './navigation'

/** 分面维度的展示顺序与文案（键集合由后端保证，顺序与命名归前端）。
 *  仅作 URL ?facet= 白名单校验与 meta 行文案——树上不展示全维度：
 *  影视走 is_tv→主维度 分层（见 PRIMARY_DIMENSION），图书/音乐为
 *  主维度单层值叶 */
export const DIMENSIONS: Record<
  MarksCategory,
  readonly { key: string; label: string }[]
> = {
  movie: [
    { key: 'genre', label: '类型' },
    { key: 'is_tv', label: '形态' },
    { key: 'country', label: '国家/地区' },
    { key: 'decade', label: '年代' },
    { key: 'director', label: '导演' },
  ],
  book: [
    { key: 'press', label: '出版社' },
    { key: 'author', label: '作者' },
    { key: 'translator', label: '译者' },
    { key: 'decade', label: '年代' },
  ],
  music: [
    { key: 'genre', label: '流派' },
    { key: 'singer', label: '歌手' },
    { key: 'decade', label: '年代' },
  ],
}

/** 每类别的树展示主维度：影视=类型（挂在 is_tv 形态层下，见树构建）、
 *  图书=出版社（图书无类型/分类数据——rexxar tags 仅 2/55 有值、丛书
 *  聚合 ≤2，出版社是 book 元数据里头部聚合力最强的内容谱系维度）、
 *  音乐=流派 */
export const PRIMARY_DIMENSION: Record<
  MarksCategory,
  { key: string; label: string }
> = {
  book: { key: 'press', label: '出版社' },
  movie: { key: 'genre', label: '类型' },
  music: { key: 'genre', label: '流派' },
}

/** 影视的形态层维度（is_tv 派生：电影/电视剧/未分类），树中位于
 *  类别根与主维度之间；其余类别无此层 */
export const KIND_DIMENSION: { key: 'is_tv'; label: string } | null =
  { key: 'is_tv', label: '形态' }

/** 维度键 → 展示名（查不到时回退键本身） */
export function facetDimLabel(
  category: MarksCategory,
  dim: string,
): string {
  return DIMENSIONS[category].find((d) => d.key === dim)?.label ?? dim
}

/** 后端长尾合并的保留字面量（计数 1 的值聚合进它，仅影视类型与图书
 *  出版社开启），与后端 facets::OTHER_VALUE 对齐 */
export const OTHER_FACET_VALUE = '__other__'

/** 值的展示名：保留字面量 → 「其他+维度名」（其他类型/其他出版社），
 *  其余原样 */
export function facetValueLabel(
  category: MarksCategory,
  dim: string,
  value: string,
): string {
  return value === OTHER_FACET_VALUE
    ? `其他${facetDimLabel(category, dim)}`
    : value
}

/** 一步筛选（dim/value 均为后端原始值，不转义） */
export type FacetStep = {
  dim: string
  value: string
}

/** 树标签值含 / 会破坏 tag 路径的层级语义，替换为全角（树构建与
 *  选中指示共用） */
export function escapeFacetValue(value: string): string {
  return value.replaceAll('/', '／')
}

/** 路径 → 请求/URL 用的 facet 参数串（dim=value;dim=value，顺序即步骤） */
export function facetPathKey(path: readonly FacetStep[]): string {
  return path.map((step) => `${step.dim}=${step.value}`).join(';')
}

/** URL ?facet= 参数 → 筛选步骤（顺序保留）。按 ; 拆步逐步校验维度
 *  白名单——非法步起整体截断（手改 URL 容错保留合法前缀）；同维度
 *  重复只留首见。侧边栏树与列表页共享的解析入口，后端仍兜底校验 */
export function parseFacetPath(
  category: MarksCategory,
  raw: string | null,
): FacetStep[] {
  if (!raw) return []
  const seen = new Set<string>()
  const steps: FacetStep[] = []
  for (const part of raw.split(';')) {
    const eq = part.indexOf('=')
    if (eq <= 0) break
    const dim = part.slice(0, eq)
    const value = part.slice(eq + 1)
    if (!value) break
    if (!DIMENSIONS[category].some((d) => d.key === dim)) break
    if (seen.has(dim)) continue
    seen.add(dim)
    steps.push({ dim, value })
  }
  return steps
}
