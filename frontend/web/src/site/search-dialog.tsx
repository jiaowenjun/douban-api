import { useState } from 'react'
import { Search } from 'lucide-react'

import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogPopup,
  DialogTitle,
} from 'weimo-ui-core/components/coss/dialog'
import { FrostedIconButton } from 'weimo-ui-core/components/frosted-icon-button'
import { TextButton } from 'weimo-ui-core/components/text-button'

/** 顶栏搜索入口：顶栏右侧原主题切换按钮位（App 处 showThemeToggle={false}
 *  让位）改为搜索按钮，弹窗暂为占位——搜索功能待实现 */
export function TopBarSearch() {
  const [open, setOpen] = useState(false)

  return (
    <>
      <FrostedIconButton
        aria-label="搜索"
        onClick={() => setOpen(true)}
        tooltip="搜索"
      >
        <Search aria-hidden="true" />
      </FrostedIconButton>
      <Dialog onOpenChange={setOpen} open={open}>
        <DialogPopup className="search-dialog">
          <DialogHeader>
            <DialogTitle>搜索</DialogTitle>
            <DialogDescription>搜索功能待实现</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <TextButton onClick={() => setOpen(false)} type="button">
              知道了
            </TextButton>
          </DialogFooter>
        </DialogPopup>
      </Dialog>
    </>
  )
}
