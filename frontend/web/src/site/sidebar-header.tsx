import { Settings, UserRound } from 'lucide-react'

import { GhostIconButton } from 'weimo-ui-core/components/ghost-icon-button'

import { resolveApiUrl } from './api'
import { useDoubanUser } from './douban-user'
import { useSiteSettings } from './site-settings'
import './sidebar-header.css'

/** 侧边栏身份栏：左侧头像 + 用户名，右侧 ID/设置入口（GhostIconButton），
    样式对齐 weimo-biji 的 biji-side-bar__header；统计区/导航/筛选树
    由 SiteSidebar 主体区承接；两个入口都开弹层对话框（豆瓣 ID 归
    DoubanUserProvider、设置归 SiteSettingsProvider），抽屉保持原样 */
export function SidebarHeader() {
  const { uid, profile, openLogin } = useDoubanUser()
  const { openSettings } = useSiteSettings()

  return (
    <div className="sidebar-header">
        <div className="sidebar-header__identity">
          {profile?.avatar ? (
            <img
              alt=""
              aria-hidden="true"
              className="sidebar-header__avatar"
              src={resolveApiUrl(profile.avatar)}
            />
          ) : (
            <UserRound aria-hidden="true" />
          )}
          <span
            className={
              uid
                ? 'sidebar-header__username'
                : 'sidebar-header__username sidebar-header__username--empty'
            }
            title={uid ?? undefined}
          >
            {uid ? profile?.username || uid : '未输入'}
          </span>
        </div>
        <div className="sidebar-header__actions">
          <GhostIconButton
            aria-label={
              uid ? `豆瓣 ID：${uid}，点击管理` : '输入豆瓣 ID'
            }
            className="sidebar-header__button"
            onClick={openLogin}
            size="sm"
            tooltip={
              uid
                ? profile?.username
                  ? `${profile.username}（${uid}）`
                  : uid
                : '输入豆瓣 ID'
            }
          >
            <UserRound aria-hidden="true" />
          </GhostIconButton>
          <GhostIconButton
            aria-label="打开设置"
            className="sidebar-header__button"
            onClick={openSettings}
            size="sm"
            tooltip="设置"
          >
            <Settings aria-hidden="true" />
          </GhostIconButton>
        </div>
    </div>
  )
}
