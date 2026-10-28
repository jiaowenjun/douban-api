import { LogIn, UserRound } from 'lucide-react'

import { GhostIconButton } from 'weimo-ui-core/components/ghost-icon-button'

import { useDoubanUser } from './douban-user'
import { SidebarDebugSwitch } from './sidebar-debug-switch'
import './sidebar-header.css'

type SidebarHeaderProps = {
  siteName: string
}

/** 侧边栏顶部栏：站点名 + 登录入口（登录后显示豆瓣用户名与头像），
    样式对齐 weimo-biji 的 biji-side-bar__header；
    附带钉在侧边栏底部的 DEBUG 子容器描边开关 */
export function SidebarHeader({ siteName }: SidebarHeaderProps) {
  const { uid, profile, openLogin } = useDoubanUser()

  return (
    <>
      <div className="sidebar-header">
        <div className="sidebar-header__identity">{siteName}</div>
        <div className="sidebar-header__actions">
          {uid ? (
            <span className="sidebar-header__username" title={uid}>
              {profile?.username || uid}
            </span>
          ) : null}
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
            {profile?.avatar ? (
              <img
                alt=""
                aria-hidden="true"
                className="sidebar-header__avatar"
                src={profile.avatar}
              />
            ) : uid ? (
              <UserRound aria-hidden="true" />
            ) : (
              <LogIn aria-hidden="true" />
            )}
          </GhostIconButton>
        </div>
      </div>
      <SidebarDebugSwitch />
    </>
  )
}
