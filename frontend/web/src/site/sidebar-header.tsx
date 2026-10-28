import { useState } from 'react'
import { LogIn, UserRound } from 'lucide-react'

import { GhostIconButton } from 'weimo-ui-core/components/ghost-icon-button'

import { useDoubanUser } from './douban-user'
import { LoginDialog } from './login-dialog'
import './sidebar-header.css'

type SidebarHeaderProps = {
  siteName: string
}

/** 侧边栏顶部栏：站点名 + 登录入口，样式对齐 weimo-biji 的 biji-side-bar__header */
export function SidebarHeader({ siteName }: SidebarHeaderProps) {
  const { uid } = useDoubanUser()
  const [loginOpen, setLoginOpen] = useState(false)

  return (
    <>
      <div className="sidebar-header">
        <div className="sidebar-header__identity">{siteName}</div>
        <div className="sidebar-header__actions">
          <GhostIconButton
            aria-label={uid ? `豆瓣账号：${uid}，点击管理` : '登录豆瓣账号'}
            className="sidebar-header__button"
            onClick={() => setLoginOpen(true)}
            size="sm"
            tooltip={uid ? uid : '登录'}
          >
            {uid ? (
              <UserRound aria-hidden="true" />
            ) : (
              <LogIn aria-hidden="true" />
            )}
          </GhostIconButton>
        </div>
      </div>
      <LoginDialog onOpenChange={setLoginOpen} open={loginOpen} />
    </>
  )
}
