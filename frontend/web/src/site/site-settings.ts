import { createContext, useContext } from 'react'

export const SITE_SETTINGS_STORAGE_KEY = 'weimo-douban:settings'

/** 折叠态标题行水平对齐：居中（默认，原版观感）/ 左对齐 */
export type CollapsedTitleAlign = 'center' | 'left'

export type SiteSettings = {
  /** 标记卡片条目元信息区（封面/豆瓣评分/摘要行）挂载时默认展开 */
  defaultMetaExpanded: boolean
  /** 折叠态标题行（solo 副本）水平对齐，纯展示即时生效 */
  collapsedTitleAlign: CollapsedTitleAlign
}

export const DEFAULT_SITE_SETTINGS: SiteSettings = {
  defaultMetaExpanded: false,
  collapsedTitleAlign: 'center',
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

/** 解析 localStorage 原始值为设置对象：坏 JSON / 非对象整体回落默认；
 *  字段级逐项容错——旧版本存档缺新字段、或某字段类型/取值非法，仅该
 *  字段回落默认，其余合法字段保留（设置项渐增，跨版本存档互不废弃） */
export function parseStoredSiteSettings(raw: string | null): SiteSettings {
  if (!raw) return DEFAULT_SITE_SETTINGS
  try {
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null) {
      return DEFAULT_SITE_SETTINGS
    }
    const record = parsed as Record<string, unknown>
    return {
      defaultMetaExpanded:
        typeof record.defaultMetaExpanded === 'boolean'
          ? record.defaultMetaExpanded
          : DEFAULT_SITE_SETTINGS.defaultMetaExpanded,
      collapsedTitleAlign:
        record.collapsedTitleAlign === 'left' ||
        record.collapsedTitleAlign === 'center'
          ? record.collapsedTitleAlign
          : DEFAULT_SITE_SETTINGS.collapsedTitleAlign,
    }
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
