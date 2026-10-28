import { useEffect, useState } from 'react'

import { LabeledSwitch } from 'weimo-ui-core/components/labeled-switch'

import './sidebar-debug-switch.css'

/** 挂到 <html> 上，供 CSS 描边侧边栏内子容器 */
const DEBUG_CLASS = 'sidebar-debug'

/** DEBUG 开关：开启后侧边栏内子容器显示 1px --color-border 描边（--radius 圆角），默认关闭 */
export function SidebarDebugSwitch() {
  const [debug, setDebug] = useState(false)

  useEffect(() => {
    document.documentElement.classList.toggle(DEBUG_CLASS, debug)

    return () => {
      document.documentElement.classList.remove(DEBUG_CLASS)
    }
  }, [debug])

  return (
    <div className="sidebar-debug-switch">
      <LabeledSwitch
        ariaLabel="切换侧边栏子容器 DEBUG 描边"
        checked={debug}
        labelOff="DEBUG"
        labelOn="DEBUG"
        onCheckedChange={setDebug}
      />
    </div>
  )
}
