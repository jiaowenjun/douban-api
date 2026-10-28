import { createContext, useContext } from 'react'

export const DOUBAN_UID_STORAGE_KEY = 'weimo-douban:uid'

export type DoubanUserValue = {
  /** 当前登录的豆瓣用户 ID，未登录为 null */
  uid: string | null
  signIn: (uid: string) => void
  signOut: () => void
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
