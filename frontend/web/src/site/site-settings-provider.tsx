import { useCallback, useMemo, useState } from 'react'
import type { ReactNode } from 'react'

import {
  SITE_SETTINGS_STORAGE_KEY,
  SiteSettingsContext,
  readStoredSiteSettings,
  type SiteSettings,
} from './site-settings'
import { SettingsDialog } from './settings-dialog'

function persistSiteSettings(settings: SiteSettings) {
  try {
    localStorage.setItem(SITE_SETTINGS_STORAGE_KEY, JSON.stringify(settings))
  } catch {
    // localStorage 不可用时仅保留内存态
  }
}

/** 站点本地偏好：存 localStorage（本机偏好随浏览器，不进后端/URL），
 *  惰性初始化读一次；更新即合并写 state + 持久化。设置对话框的开关
 *  也持在这里，openSettings 供侧边栏等入口共用（同 DoubanUserProvider
 *  挂 LoginDialog 的「对话框归 Provider」模式）。消费方式——设置对
 *  话框读写全量 settings，MarkCard 读默认展开值（挂载时初始化、变化
 *  时未手动拨过的卡片即时跟随，手动拨过的保留用户态） */
export function SiteSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<SiteSettings>(readStoredSiteSettings)
  const [settingsOpen, setSettingsOpen] = useState(false)

  const updateSettings = useCallback((partial: Partial<SiteSettings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...partial }
      persistSiteSettings(next)
      return next
    })
  }, [])

  const openSettings = useCallback(() => setSettingsOpen(true), [])

  const value = useMemo(
    () => ({ settings, updateSettings, openSettings }),
    [settings, updateSettings, openSettings],
  )

  return (
    <SiteSettingsContext.Provider value={value}>
      {children}
      <SettingsDialog onOpenChange={setSettingsOpen} open={settingsOpen} />
    </SiteSettingsContext.Provider>
  )
}
