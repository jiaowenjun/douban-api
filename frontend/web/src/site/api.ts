/** 后端 API 客户端——web 站点首次接后端（用户资料）。
 *  开发期经 Vite proxy（/api → 127.0.0.1:8080），部署时同源反向代理即可。 */

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
