import { createContext, useContext } from 'react'

export const SITE_SETTINGS_STORAGE_KEY = 'weimo-douban:settings'

export type SiteSettings = {
  /** 标记卡片条目元信息区（封面/豆瓣评分/摘要行）挂载时默认展开 */
  defaultMetaExpanded: boolean
}

export const DEFAULT_SITE_SETTINGS: SiteSettings = {
  defaultMetaExpanded: false,
}

export type SiteSettingsValue = {
  settings: SiteSettings
  updateSettings: (partial: Partial<SiteSettings>) => void
}

export const SiteSettingsContext = createContext<SiteSettingsValue | null>(null)

export function useSiteSettings(): SiteSettingsValue {
  const value = useContext(SiteSettingsContext)
  if (!value) {
    throw new Error('useSiteSettings 必须在 SiteSettingsProvider 内使用')
  }
  return value
}

/** 解析 localStorage 原始值为设置对象：坏 JSON / 非法形状整体回落默认
 *  （存档由本站写入，形状不符即视为废弃，不逐字段抢救） */
export function parseStoredSiteSettings(raw: string | null): SiteSettings {
  if (!raw) return DEFAULT_SITE_SETTINGS
  try {
    const parsed: unknown = JSON.parse(raw)
    if (
      typeof parsed !== 'object' ||
      parsed === null ||
      typeof (parsed as SiteSettings).defaultMetaExpanded !== 'boolean'
    ) {
      return DEFAULT_SITE_SETTINGS
    }
    return { defaultMetaExpanded: (parsed as SiteSettings).defaultMetaExpanded }
  } catch {
    return DEFAULT_SITE_SETTINGS
  }
}

export function readStoredSiteSettings(): SiteSettings {
  try {
    return parseStoredSiteSettings(
      localStorage.getItem(SITE_SETTINGS_STORAGE_KEY),
    )
  } catch {
    return DEFAULT_SITE_SETTINGS
  }
}
