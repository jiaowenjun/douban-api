import { LogIn, Settings, UserRound } from 'lucide-react'

import { GhostIconButton } from 'weimo-ui-core/components/ghost-icon-button'

import { useDoubanUser } from './douban-user'
import { SidebarDebugSwitch } from './sidebar-debug-switch'
import './sidebar-header.css'

/** 侧边栏顶部栏：左侧头像 + 用户名，右侧登录/设置入口（GhostIconButton），
    样式对齐 weimo-biji 的 biji-side-bar__header；
    附带钉在侧边栏底部的 DEBUG 子容器描边开关 */
export function SidebarHeader() {
  const { uid, profile, openLogin } = useDoubanUser()

  return (
    <>
      <div className="sidebar-header">
        <div className="sidebar-header__identity">
          {profile?.avatar ? (
            <img
              alt=""
              aria-hidden="true"
              className="sidebar-header__avatar"
              src={profile.avatar}
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
            {uid ? profile?.username || uid : '未登录'}
          </span>
        </div>
        <div className="sidebar-header__actions">
          <GhostIconButton
            aria-label={
              uid ? `豆瓣账号：${uid}，点击管理` : '登录豆瓣账号'
            }
            className="sidebar-header__button"
            onClick={openLogin}
            size="sm"
            tooltip={
              uid
                ? profile?.username
                  ? `${profile.username}（${uid}）`
                  : uid
                : '登录'
            }
          >
            {uid ? (
              <UserRound aria-hidden="true" />
            ) : (
              <LogIn aria-hidden="true" />
            )}
          </GhostIconButton>
          <GhostIconButton
            aria-label="打开设置"
            className="sidebar-header__button"
            size="sm"
            tooltip="设置"
          >
            <Settings aria-hidden="true" />
          </GhostIconButton>
        </div>
      </div>
      <SidebarDebugSwitch />
    </>
  )
}
