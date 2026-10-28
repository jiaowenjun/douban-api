import { useCallback, useMemo, useState } from 'react'
import type { ReactNode } from 'react'

import {
  DOUBAN_UID_STORAGE_KEY,
  DoubanUserContext,
  readStoredDoubanUid,
} from './douban-user'

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

/** 站点身份：豆瓣用户 ID 存 localStorage，后续 /users/{uid}/marks 等请求都依赖它 */
export function DoubanUserProvider({ children }: { children: ReactNode }) {
  const [uid, setUid] = useState<string | null>(readStoredDoubanUid)

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

  const value = useMemo(
    () => ({ uid, signIn, signOut }),
    [uid, signIn, signOut],
  )

  return (
    <DoubanUserContext.Provider value={value}>
      {children}
    </DoubanUserContext.Provider>
  )
}
