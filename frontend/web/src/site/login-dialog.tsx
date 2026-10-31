import { useState } from 'react'
import { useNavigate } from 'react-router'
import { UserRound } from 'lucide-react'

import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPanel,
  DialogPopup,
  DialogTitle,
} from 'weimo-ui-core/components/coss/dialog'
import { TextButton } from 'weimo-ui-core/components/text-button'

import { resolveApiUrl } from './api'
import { useDoubanUser } from './douban-user'
import './login-dialog.css'

type LoginDialogProps = {
  onOpenChange: (open: boolean) => void
  open: boolean
}

/** 登录对话框：输入豆瓣用户 ID 作为站点身份，已登录时可切换或退出；
 *  底部「返回」关框并退回欢迎页（未登录误入内容页的退路） */
export function LoginDialog({ onOpenChange, open }: LoginDialogProps) {
  const { uid, profile, signIn, signOut } = useDoubanUser()
  const navigate = useNavigate()
  const [draft, setDraft] = useState('')
  // 每次打开时把草稿重置为当前 uid（render 期调整，避免 effect 级联渲染）
  const [prevOpen, setPrevOpen] = useState(open)
  if (open !== prevOpen) {
    setPrevOpen(open)
    if (open) {
      setDraft(uid ?? '')
    }
  }

  const normalized = draft.trim()

  function confirmSignIn() {
    if (!normalized) return
    signIn(normalized)
    onOpenChange(false)
  }

  function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    confirmSignIn()
  }

  // input 的 Enter 不走隐式提交（部分 webview 不触发），显式处理
  function handleInputKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key !== 'Enter') return
    event.preventDefault()
    confirmSignIn()
  }

  function handleSignOut() {
    signOut()
    onOpenChange(false)
  }

  function handleBack() {
    onOpenChange(false)
    navigate('/')
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogPopup className="login-dialog">
        <form className="login-dialog__form" onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>登录</DialogTitle>
            <DialogDescription>
              输入你的豆瓣用户 ID（个人主页地址 people/
              后的那段字符），用于同步并展示你的标记数据。
            </DialogDescription>
          </DialogHeader>
          <DialogPanel>
            {uid ? (
              <div className="login-dialog__account">
                {profile?.avatar ? (
                  <img
                    alt=""
                    aria-hidden="true"
                    className="login-dialog__account-avatar"
                    src={resolveApiUrl(profile.avatar)}
                  />
                ) : (
                  <UserRound
                    aria-hidden="true"
                    className="login-dialog__account-avatar"
                  />
                )}
                <span className="login-dialog__account-name">
                  {profile?.username || uid}
                </span>
                <span className="login-dialog__account-uid">@{uid}</span>
              </div>
            ) : null}
            <label className="login-dialog__field">
              <span className="login-dialog__label">豆瓣 ID</span>
              <input
                className="login-dialog__input"
                name="douban-uid"
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={handleInputKeyDown}
                placeholder="如：wenjun30"
                type="text"
                value={draft}
              />
            </label>
          </DialogPanel>
          <DialogFooter>
            <TextButton onClick={handleBack} type="button" variant="ghost">
              返回
            </TextButton>
            {uid ? (
              <TextButton onClick={handleSignOut} type="button" variant="ghost">
                退出登录
              </TextButton>
            ) : null}
            <TextButton disabled={!normalized} type="submit">
              {uid ? '切换账号' : '登录'}
            </TextButton>
          </DialogFooter>
        </form>
      </DialogPopup>
    </Dialog>
  )
}
