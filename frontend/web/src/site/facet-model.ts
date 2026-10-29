import type { MarksCategory } from './navigation'

/** 分面维度的展示顺序与文案（键集合由后端保证，顺序与命名归前端）。
 *  仅作 URL ?facet= 白名单校验与 meta 行文案——树上不展示全维度，
 *  树的二级层每类别只取一个主维度（见 PRIMARY_DIMENSION） */
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

/** 每类别的树展示主维度（二级树的唯一值叶层）：电影=类型、图书=
 *  出版社（图书无类型/分类数据——rexxar tags 仅 2/55 有值、丛书聚合
 *  ≤2，出版社是 book 元数据里头部聚合力最强的内容谱系维度）、
 *  音乐=流派 */
export const PRIMARY_DIMENSION: Record<
  MarksCategory,
  { key: string; label: string }
> = {
  book: { key: 'press', label: '出版社' },
  movie: { key: 'genre', label: '类型' },
  music: { key: 'genre', label: '流派' },
}

/** 维度键 → 展示名（查不到时回退键本身） */
export function facetDimLabel(
  category: MarksCategory,
  dim: string,
): string {
  return DIMENSIONS[category].find((d) => d.key === dim)?.label ?? dim
}

/** 后端长尾合并的保留字面量（计数 1 的值聚合进它，仅电影类型与图书
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

/** 当前选中的分面过滤（dim/value 均为后端原始值，不转义） */
export type FacetSelection = {
  dim: string
  value: string
}

/** 树上叶子的完整分面条件：含所属类别（三类别常驻树中跨类别叶子
 *  需要类别信息才能决定改参数还是跳路由） */
export type FacetLeaf = FacetSelection & { category: MarksCategory }

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
