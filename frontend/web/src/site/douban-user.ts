import { createContext, useContext } from 'react'

import type { DoubanProfile } from './api'

export const DOUBAN_UID_STORAGE_KEY = 'weimo-douban:uid'

export type DoubanUserValue = {
  /** 当前登录的豆瓣用户 ID，未登录为 null */
  uid: string | null
  /** 登录用户的豆瓣资料（头像/用户名）；未登录或后端未同步过该用户时为 null */
  profile: DoubanProfile | null
  signIn: (uid: string) => void
  signOut: () => void
  /** 打开登录对话框（对话框由 Provider 持有，任意入口共用一份开关） */
  openLogin: () => void
}

export const DoubanUserContext = createContext<DoubanUserValue | null>(null)

export function useDoubanUser(): DoubanUserValue {
  const value = useContext(DoubanUserContext)
  if (!value) {
    throw new Error('useDoubanUser 必须在 DoubanUserProvider 内使用')
  }
  return value
}

export function readStoredDoubanUid(): string | null {
  try {
    return localStorage.getItem(DOUBAN_UID_STORAGE_KEY)?.trim() || null
  } catch {
    return null
  }
}
