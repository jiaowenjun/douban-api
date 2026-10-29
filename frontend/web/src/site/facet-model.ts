import type { MarksCategory } from './navigation'

/** 分面维度的展示顺序与文案（键集合由后端保证，顺序与命名归前端） */
export const DIMENSIONS: Record<
  MarksCategory,
  readonly { key: string; label: string }[]
> = {
  movie: [
    { key: 'genre', label: '类型' },
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

/** 维度键 → 展示名（查不到时回退键本身） */
export function facetDimLabel(
  category: MarksCategory,
  dim: string,
): string {
  return DIMENSIONS[category].find((d) => d.key === dim)?.label ?? dim
}

/** 当前选中的分面过滤（dim/value 均为后端原始值，不转义） */
export type FacetSelection = {
  dim: string
  value: string
}

/** URL ?facet= 参数（dim=value）→ 选择；维度不在该类别白名单或格式
 *  非法返回 null（侧边栏树与列表页共享 URL 的解析入口，手改 URL 的
 *  容错由前端先行拦截，后端仍兜底校验） */
export function parseFacetParam(
  category: MarksCategory,
  raw: string | null,
): FacetSelection | null {
  if (!raw) return null
  const eq = raw.indexOf('=')
  if (eq <= 0) return null
  const dim = raw.slice(0, eq)
  const value = raw.slice(eq + 1)
  if (!value) return null
  return DIMENSIONS[category].some((d) => d.key === dim) ? { dim, value } : null
}
