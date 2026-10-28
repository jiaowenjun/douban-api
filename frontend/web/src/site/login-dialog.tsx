import { useState } from 'react'

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

import { useDoubanUser } from './douban-user'
import './login-dialog.css'

type LoginDialogProps = {
  onOpenChange: (open: boolean) => void
  open: boolean
}

/** 登录对话框：输入豆瓣用户 ID 作为站点身份，已登录时可切换或退出 */
export function LoginDialog({ onOpenChange, open }: LoginDialogProps) {
  const { uid, signIn, signOut } = useDoubanUser()
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
