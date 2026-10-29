import { useCallback, useMemo, useState } from 'react'
import type { ReactNode } from 'react'

import { queryClient } from '../query-client'
import {
  DOUBAN_UID_STORAGE_KEY,
  DoubanUserContext,
  readStoredDoubanUid,
} from './douban-user'
import { doubanQueryKeys, useDoubanProfileQuery } from './douban-queries'
import { LoginDialog } from './login-dialog'

function persistDoubanUid(uid: string | null) {
  try {
    if (uid) {
      localStorage.setItem(DOUBAN_UID_STORAGE_KEY, uid)
    } else {
      localStorage.removeItem(DOUBAN_UID_STORAGE_KEY)
    }
  } catch {
    // localStorage 不可用时仅保留内存态
  }
}

/** 站点身份：豆瓣用户 ID 存 localStorage，后续 /users/{uid}/marks 等请求都依赖它。
 *  登录对话框的开关也持在这里，侧边栏入口与内容页的登录引导共用一份。
 *  资料经 react-query 缓存（key 带 uid 作用域，切换 uid 即换缓存）；
 *  切号/登出时清空全部查询缓存（对齐 biji logout 清缓存惯例） */
export function DoubanUserProvider({ children }: { children: ReactNode }) {
  const [uid, setUid] = useState<string | null>(readStoredDoubanUid)
  const [loginOpen, setLoginOpen] = useState(false)

  // 后端未同步过该用户或不可达时缓存为 null，界面回退 uid 展示，
  // 不阻塞登录态
  const profileQuery = useDoubanProfileQuery(uid)
  const profile = profileQuery.data ?? null

  const signIn = useCallback(
    (nextUid: string) => {
      const normalized = nextUid.trim()
      if (!normalized) return
      if (normalized !== uid) {
        queryClient.removeQueries({ queryKey: doubanQueryKeys.all })
      }
      setUid(normalized)
      persistDoubanUid(normalized)
    },
    [uid],
  )

  const signOut = useCallback(() => {
    queryClient.removeQueries({ queryKey: doubanQueryKeys.all })
    setUid(null)
    persistDoubanUid(null)
  }, [])

  const openLogin = useCallback(() => setLoginOpen(true), [])

  const value = useMemo(
    () => ({ uid, profile, signIn, signOut, openLogin }),
    [uid, profile, signIn, signOut, openLogin],
  )

  return (
    <DoubanUserContext.Provider value={value}>
      {children}
      <LoginDialog onOpenChange={setLoginOpen} open={loginOpen} />
    </DoubanUserContext.Provider>
  )
}
