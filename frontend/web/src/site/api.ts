/** 后端 API 客户端——web 站点首次接后端（用户资料）。
 *  开发期经 Vite proxy（/api → 127.0.0.1:8080），部署时同源反向代理即可。 */

import { MARKS_STATUS } from './navigation'

export type DoubanProfile = {
  uid: string
  username: string
  /** 本地封面代理相对路径（/api/v1/covers?u=...），豆瓣头像直连会被防盗链拦（418） */
  avatar: string
}

/** 拉取豆瓣用户资料；后端不可达或该 uid 尚未同步过（404）时返回 null，不抛错 */
export async function fetchDoubanProfile(uid: string): Promise<DoubanProfile | null> {
  try {
    const res = await fetch(`/api/v1/users/${encodeURIComponent(uid)}/profile`)
    if (!res.ok) return null
    return (await res.json()) as DoubanProfile
  } catch {
    return null
  }
}

// --------------------------------------------------------------------------- //
// /users/{uid}/marks
// --------------------------------------------------------------------------- //

/** 条目结构化元信息（?expand=subject 时随每条标记返回；形状与后端
 *  subject_view 一致）。source: rexxar = 富集缓存（有豆瓣评分）；
 *  intro = 未富集时由 records.intro 启发式合成的兜底（字段可能稀疏） */
export type DoubanSubject = {
  category: string
  subject_id: string
  title: string
  original_title: string
  aka: string[]
  year: number | null
  rating: { value: number | null; count: number | null }
  /** 本地封面代理相对路径（rexxar 中图，比标记行小图清晰） */
  cover: string
  url: string
  status: string
  fetched_at: string | null
  source: 'rexxar' | 'intro'
  movie?: {
    directors: string[]
    actors: string[]
    genres: string[]
    countries: string[]
    languages: string[]
    durations: string[]
    pubdate: string[]
    is_tv: boolean | null
    episodes_count: number | null
  }
  book?: {
    authors: string[]
    translators: string[]
    press: string[]
    pubdate: string[]
    pages: string[]
    subtitle: string
    series: string
  }
  music?: {
    singers: string[]
    genres: string[]
    media: string[]
    discs: string[]
    pubdate: string[]
    publisher: string[]
    version: string[]
    songs: string[]
  }
}

/** 一条书影音标记（字段名与后端 Record 序列化一致，snake_case） */
export type DoubanMark = {
  category: string
  status: string
  title: string
  alt_title: string
  /** 我的评分 1-5 星；未评过为 null */
  my_rating: number | null
  /** 标记日期（YYYY-MM-DD） */
  date: string
  comment: string
  subject_url: string
  subject_id: string
  /** 本地封面代理相对路径（/api/v1/covers?u=...），空串表示无封面 */
  cover: string
  comment_id: string
  intro: string
  /** 条目结构化元信息（expand=subject）；旧版响应无此字段 */
  subject?: DoubanSubject | null
}

export type DoubanMarksPage = {
  user: string
  category: string
  status: string
  /** 上次抓取时间（RFC3339） */
  cached_at: string
  fresh: boolean
  total: number
  page: number
  page_size: number
  items: DoubanMark[]
}

export type MarksResult =
  | { ok: true; data: DoubanMarksPage }
  | { ok: false; kind: 'not-cached' | 'error'; message: string }

/** 拉取某维度（category+status）的标记页；只读缓存，不触发抓取。
 *  category 另接受 'all'：书影音三类别按日期归并的合并视图
 *  （后端任一类别已同步即放行，不支持 facet 过滤）。
 *  附 expand=subject：每条标记带条目结构化元信息（已富集用 rexxar 数据，
 *  未富集回退 intro 启发式合成，旧版后端不认该参数）。
 *  facet 为可选分面过滤（格式 dim=value，多个条件用 ; 分隔，后端按类别
 *  白名单校验；同维度 OR、跨维度 AND），过滤后 total/分页均按命中集计。
 *  404 DIMENSION_NOT_CACHED（该用户该维度尚未同步过）单列为 not-cached，
 *  其余失败（网络/限流/鉴权等）归为 error，均不抛错 */
export async function fetchDoubanMarks(
  uid: string,
  category: string,
  page = 1,
  pageSize = 20,
  facet?: string | null,
): Promise<MarksResult> {
  const params = new URLSearchParams({
    category,
    status: MARKS_STATUS,
    page: String(page),
    page_size: String(pageSize),
    expand: 'subject',
  })
  if (facet) {
    params.set('facet', facet)
  }
  try {
    const res = await fetch(
      `/api/v1/users/${encodeURIComponent(uid)}/marks?${params}`,
    )
    if (res.ok) {
      return { ok: true, data: (await res.json()) as DoubanMarksPage }
    }
    let kind: 'not-cached' | 'error' = 'error'
    let message = `请求失败（HTTP ${res.status}）`
    try {
      const body = await res.json()
      if (body?.error?.code === 'DIMENSION_NOT_CACHED') kind = 'not-cached'
      if (body?.error?.message) message = body.error.message
    } catch {
      // 非 JSON 错误体，保留默认 message
    }
    return { ok: false, kind, message }
  } catch {
    return { ok: false, kind: 'error', message: '网络错误，请稍后重试' }
  }
}

// --------------------------------------------------------------------------- //
// /users/{uid}/facets
// --------------------------------------------------------------------------- //

/** 单个分面值及其计数（计数 = 元信息含该值的标记条数） */
export type DoubanFacetValue = {
  value: string
  count: number
}

/** 分面聚合响应；facets 键随类别——movie: genre/country/director/decade，
 *  book: press/author/translator/decade，music: genre/singer/decade。
 *  值列表按计数降序、同计数按值升序（后端排序） */
export type DoubanFacets = {
  user: string
  category: string
  status: string
  /** 该维度标记总数（未按分面过滤） */
  total: number
  facets: Record<string, DoubanFacetValue[]>
}

/** 拉取某维度的分面聚合（分面树数据源，只读缓存）；可选 facet 条件
 *  （dim=value;… 与 marks 端点同语法）返回条件计数，total 为命中数——
 *  影视形态层分支内的计数数据源。维度未同步或请求失败一律返回
 *  null——树该分支静默隐藏，不打断主流程 */
export async function fetchDoubanFacets(
  uid: string,
  category: string,
  facet?: string | null,
): Promise<DoubanFacets | null> {
  try {
    const params = new URLSearchParams({ category, status: MARKS_STATUS })
    if (facet) {
      params.set('facet', facet)
    }
    const res = await fetch(
      `/api/v1/users/${encodeURIComponent(uid)}/facets?${params}`,
    )
    if (!res.ok) return null
    return (await res.json()) as DoubanFacets
  } catch {
    return null
  }
}

// --------------------------------------------------------------------------- //
// /users/{uid}/stats
// --------------------------------------------------------------------------- //

/** 按日计数（date 为 YYYY-MM-DD），形状即 Heatmap 的 dailyCounts */
export type DoubanDailyCount = {
  date: string
  count: number
}

/** 侧边栏统计聚合；totals 三类恒出现（未同步的类别为 0） */
export type DoubanStats = {
  user: string
  status: string
  totals: { movie: number; book: number; music: number }
  daily_counts: DoubanDailyCount[]
}

/** 拉取标记统计（按类别计数 + 按日计数，只读缓存）；请求失败返回
 *  null——侧边栏统计区静默隐藏，不打断站点主流程 */
export async function fetchDoubanStats(uid: string): Promise<DoubanStats | null> {
  try {
    const params = new URLSearchParams({ status: MARKS_STATUS })
    const res = await fetch(
      `/api/v1/users/${encodeURIComponent(uid)}/stats?${params}`,
    )
    if (!res.ok) return null
    return (await res.json()) as DoubanStats
  } catch {
    return null
  }
}
