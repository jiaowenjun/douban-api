import { ExternalLink } from 'lucide-react'

import { GhostIconButton } from 'weimo-ui-core/components/ghost-icon-button'

import './sidebar-header.css'

const DOUBAN_HOME_URL = 'https://www.douban.com'

type SidebarHeaderProps = {
  siteName: string
}

/** 侧边栏顶部栏：站点名 + 跳转豆瓣入口，样式对齐 weimo-biji 的 biji-side-bar__header */
export function SidebarHeader({ siteName }: SidebarHeaderProps) {
  return (
    <div className="sidebar-header">
      <div className="sidebar-header__identity">{siteName}</div>
      <div className="sidebar-header__actions">
        <GhostIconButton
          aria-label="去豆瓣"
          className="sidebar-header__button"
          onClick={() => window.open(DOUBAN_HOME_URL, '_blank', 'noopener')}
          size="sm"
          tooltip="去豆瓣"
        >
          <ExternalLink aria-hidden="true" />
        </GhostIconButton>
      </div>
    </div>
  )
}
