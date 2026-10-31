import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { LabeledSwitch } from 'weimo-ui-core/components/labeled-switch'

import { useSiteSettings } from '../site-settings'
import './settings-page.css'

/** 设置页：站点本地偏好（localStorage 持久化，见 site-settings）。
 *  「卡片显示」组：条目信息初始展开态（仅影响此后挂载的卡片，返回
 *  列表页整体重挂载即全量生效，不打扰已挂载卡片的手动展开）与折叠
 *  态标题行对齐（纯展示，已挂载卡片即时生效）。设置项文案口径：名
 *  称是纯名词不含状态值（如「条目详细信息」而非「默认展开」），状
 *  态交给开关两侧状态词补全，任意档位拼读都通顺；不配说明文字，保
 *  持整站简洁克制。后续设置项在此页按分组追加 */
export function SettingsPage() {
  const { settings, updateSettings } = useSiteSettings()

  return (
    <section className="settings-page">
      <div className={getCardSurfaceClassName('settings-page__panel')}>
        <div className="settings-page__group-title">卡片显示</div>
        <div className="settings-page__item">
          <span className="settings-page__item-name">条目详细信息</span>
          <LabeledSwitch
            ariaLabel="条目详细信息"
            checked={settings.defaultMetaExpanded}
            labelOff="默认折叠"
            labelOn="默认展开"
            onCheckedChange={(checked) =>
              updateSettings({ defaultMetaExpanded: checked })
            }
          />
        </div>
        <div className="settings-page__item">
          <span className="settings-page__item-name">折叠时标题对齐</span>
          <LabeledSwitch
            ariaLabel="折叠时标题对齐"
            checked={settings.collapsedTitleAlign !== 'left'}
            labelOff="左对齐"
            labelOn="居中"
            onCheckedChange={(checked) =>
              updateSettings({ collapsedTitleAlign: checked ? 'center' : 'left' })
            }
          />
        </div>
      </div>
    </section>
  )
}
