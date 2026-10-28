import { useCallback, useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'

import {
  DOUBAN_UID_STORAGE_KEY,
  DoubanUserContext,
  readStoredDoubanUid,
} from './douban-user'
import { LoginDialog } from './login-dialog'
import { fetchDoubanProfile } from './api'
import type { DoubanProfile } from './api'

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
 *  登录对话框的开关也持在这里，侧边栏入口与内容页的登录引导共用一份 */
export function DoubanUserProvider({ children }: { children: ReactNode }) {
  const [uid, setUid] = useState<string | null>(readStoredDoubanUid)
  const [loginOpen, setLoginOpen] = useState(false)
  // 资料与它所属的 uid 绑定存储；展示值在渲染期派生，登出/切换后旧资料自然失效
  const [profileEntry, setProfileEntry] = useState<{
    uid: string
    profile: DoubanProfile | null
  } | null>(null)

  // uid 变化即拉豆瓣资料（头像/用户名）；后端未同步过该用户或不可达时
  // 静默落 null，界面回退 uid 展示，不阻塞登录态
  useEffect(() => {
    if (!uid) return
    let alive = true
    fetchDoubanProfile(uid).then((next) => {
      if (alive) setProfileEntry({ uid, profile: next })
    })
    return () => {
      alive = false
    }
  }, [uid])

  const profile = profileEntry?.uid === uid ? profileEntry.profile : null

  const signIn = useCallback((nextUid: string) => {
    const normalized = nextUid.trim()
    if (!normalized) return
    setUid(normalized)
    persistDoubanUid(normalized)
  }, [])

  const signOut = useCallback(() => {
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
