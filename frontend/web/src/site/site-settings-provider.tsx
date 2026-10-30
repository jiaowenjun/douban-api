import { useCallback, useMemo, useState } from 'react'
import type { ReactNode } from 'react'

import {
  SITE_SETTINGS_STORAGE_KEY,
  SiteSettingsContext,
  readStoredSiteSettings,
  type SiteSettings,
} from './site-settings'

function persistSiteSettings(settings: SiteSettings) {
  try {
    localStorage.setItem(SITE_SETTINGS_STORAGE_KEY, JSON.stringify(settings))
  } catch {
    // localStorage 不可用时仅保留内存态
  }
}

/** 站点本地偏好：存 localStorage（本机偏好随浏览器，不进后端/URL），
 *  惰性初始化读一次；更新即合并写 state + 持久化。消费方式——设置页
 *  读写全量 settings，MarkCard 等只读初始默认值（useState 初始化器，
 *  设置变化不影响已挂载卡片的手动展开态，重挂载后生效） */
export function SiteSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<SiteSettings>(readStoredSiteSettings)

  const updateSettings = useCallback((partial: Partial<SiteSettings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...partial }
      persistSiteSettings(next)
      return next
    })
  }, [])

  const value = useMemo(
    () => ({ settings, updateSettings }),
    [settings, updateSettings],
  )

  return (
    <SiteSettingsContext.Provider value={value}>
      {children}
    </SiteSettingsContext.Provider>
  )
}
