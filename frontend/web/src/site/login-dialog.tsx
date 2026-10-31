import { useState } from 'react'
import { useNavigate } from 'react-router'
import { UserRound } from 'lucide-react'

import {
  Dialog,
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

/** 豆瓣 ID 对话框：输入豆瓣用户 ID 作为站点身份（全程免账号密码，与
 *  欢迎页「免登录」口径一致），已填时可切换或清除；底部「返回」关框
 *  并退回欢迎页（未填误入内容页的退路） */
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
            <DialogTitle>请输入你的豆瓣 ID</DialogTitle>
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
            <TextButton
              className="login-dialog__back"
              onClick={handleBack}
              type="button"
              variant="ghost"
            >
              返回
            </TextButton>
            {uid ? (
              <TextButton onClick={handleSignOut} type="button" variant="ghost">
                清除 ID
              </TextButton>
            ) : null}
            <TextButton disabled={!normalized} type="submit">
              保存
            </TextButton>
          </DialogFooter>
        </form>
      </DialogPopup>
    </Dialog>
  )
}
