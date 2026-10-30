/** 后端 API 客户端——web 站点首次接后端（用户资料）。
 *  开发期经 Vite proxy（/api → 127.0.0.1:8080），部署时同源反向代理即可。 */

import { MARKS_STATUS } from './navigation'
import type { CacheVersions } from './cache-version'
import { queryClient } from '../query-client'

export async function fetchCacheVersions(uid: string): Promise<CacheVersions> {
  const response = await fetch(`/api/v1/users/${encodeURIComponent(uid)}/cache-version?status=${MARKS_STATUS}`)
  if (!response.ok) throw new Error(`版本检查失败（HTTP ${response.status}）`)
  const data = await response.json()
  if (!data || !['profile', 'stats', 'movie', 'book', 'music', 'all'].every((key) => typeof data[key] === 'string')) {
    throw new Error('版本数据格式不正确')
  }
  return data as CacheVersions
}

export type DoubanProfile = {
  uid: string
  username: string
  /** 本地封面代理相对路径（/api/v1/covers?u=...），豆瓣头像直连会被防盗链拦（418） */
  avatar: string
}

/** Request a published revision; failures stay outside the persistent query cache. */
async function fetchVersionedJson<T>(uid: string, endpoint: string, params: URLSearchParams, version?: string): Promise<T | null> {
  if (version && version !== 'unversioned' && version !== 'pending') params.set('cache_version', version)
  try {
    const response = await fetch(`/api/v1/users/${encodeURIComponent(uid)}/${endpoint}?${params}`)
    if (!response.ok) {
      if (response.status === 409) void queryClient.invalidateQueries({ queryKey: ['douban', uid, 'versions'] })
      return null
    }
    return await response.json() as T
  } catch { return null }
}

/** 用户资料；失败返回 null，由查询层保留旧数据并提供重试。 */
export function fetchDoubanProfile(uid: string, version?: string): Promise<DoubanProfile | null> {
  return fetchVersionedJson(uid, 'profile', new URLSearchParams(), version)
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
 *  （后端任一类别已同步即放行，不支持 facet 过滤；rating 为可选的
 *  我的评分筛选键 'N'/'lteN'，仅 all 视图支持）。
 *  附 expand=subject：每条标记带条目结构化元信息（已富集用 rexxar 数据，
 *  未富集回退 intro 启发式合成，旧版后端不认该参数）。
 *  ids 为可选的当页条目 id 集合（前端 dims 保序筛出的切片，顺序权威
 *  在前端）：提供时后端按传入顺序返回，忽略 page/page_size。
 *  404 DIMENSION_NOT_CACHED（该用户该维度尚未同步过）单列为 not-cached，
 *  其余失败（网络/限流/鉴权等）归为 error，均不抛错 */
export async function fetchDoubanMarks(
  uid: string,
  category: string,
  page = 1,
  pageSize = 20,
  ids?: readonly string[] | null,
  rating?: string | null,
  version?: string,
): Promise<MarksResult> {
  const params = new URLSearchParams({
    category,
    status: MARKS_STATUS,
    page: String(ids?.length ? 1 : page),
    page_size: String(ids?.length ? ids.length : pageSize),
    expand: 'subject',
  })
  if (version && version !== 'unversioned' && version !== 'pending') params.set('cache_version', version)
  if (ids?.length) {
    params.set('ids', ids.join(','))
  }
  if (rating) {
    params.set('rating', String(rating))
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
      if (body?.error?.code === 'CACHE_VERSION_CHANGED') {
        void queryClient.invalidateQueries({ queryKey: ['douban', uid, 'versions'] })
      }
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
// /users/{uid}/subject-dims（条目维度数据，标签树纯数据源）
// --------------------------------------------------------------------------- //

/** 单条目的维度投影（后端只投影原始字段：数组原文/布尔/年份；派生与
 *  聚合在前端 facet-engine 做——is_tv 布尔→电影/电视剧/未分类、
 *  year→decade 均为前端语义）。字段随类别出现，缺省即无值 */
export type DoubanDimsItem = {
  id: string
  /** 标记日期（YYYY-MM-DD）；items 按 date DESC 稳定序输出——排序契约 */
  date: string
  /** 我的评分 1-5 星（标记级字段平铺在 item 顶层，未评为 null；旧版
   *  响应无此字段）——「全部标记」导航子节点按它跨类别计数 */
  my_rating?: number | null
  dims: {
    year?: number
    is_tv?: boolean
    genres?: string[]
    countries?: string[]
    directors?: string[]
    press?: string[]
    authors?: string[]
    translators?: string[]
    singers?: string[]
  }
}

export type DoubanDims = {
  user: string
  category: string
  status: string
  total: number
  items: DoubanDimsItem[]
}

/** 拉取某维度的全部条目维度数据（轻量：每条 ~200B，聚合/筛选全在前
 *  端）；维度未同步或请求失败返回 null——树该分支静默隐藏 */
export function fetchDoubanSubjectDims(uid: string, category: string, version?: string): Promise<DoubanDims | null> {
  return fetchVersionedJson(uid, 'subject-dims', new URLSearchParams({ category, status: MARKS_STATUS }), version)
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
export function fetchDoubanStats(uid: string, version?: string): Promise<DoubanStats | null> {
  return fetchVersionedJson(uid, 'stats', new URLSearchParams({ status: MARKS_STATUS }), version)
}
