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
 *  404 DIMENSION_NOT_CACHED（该用户该维度尚未同步过）单列为 not-cached，
 *  其余失败（网络/限流/鉴权等）归为 error，均不抛错 */
export async function fetchDoubanMarks(
  uid: string,
  category: string,
  page = 1,
  pageSize = 20,
): Promise<MarksResult> {
  const params = new URLSearchParams({
    category,
    status: MARKS_STATUS,
    page: String(page),
    page_size: String(pageSize),
  })
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
