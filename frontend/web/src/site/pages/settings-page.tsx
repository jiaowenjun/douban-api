import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { LabeledSwitch } from 'weimo-ui-core/components/labeled-switch'

import { useSiteSettings } from '../site-settings'
import './settings-page.css'

/** 设置页：站点本地偏好（localStorage 持久化，见 site-settings）。
 *  「卡片显示」组：条目信息初始展开态（仅影响此后挂载的卡片，返回
 *  列表页整体重挂载即全量生效，不打扰已挂载卡片的手动展开）与折叠
 *  态标题行对齐（纯展示，已挂载卡片即时生效）。后续设置项在此页按
 *  分组追加 */
export function SettingsPage() {
  const { settings, updateSettings } = useSiteSettings()

  return (
    <section className="settings-page">
      <div className={getCardSurfaceClassName('settings-page__panel')}>
        <div className="settings-page__group-title">卡片显示</div>
        <div className="settings-page__item">
          <div className="settings-page__item-text">
            <span className="settings-page__item-name">条目信息默认展开</span>
            <p className="settings-page__item-hint">
              开启后，标记卡片的封面与豆瓣评分等信息默认展开；关闭则默认
              折叠，仍可点卡片右侧开关展开。
            </p>
          </div>
          <LabeledSwitch
            ariaLabel="条目信息默认展开"
            checked={settings.defaultMetaExpanded}
            labelOff="默认折叠"
            labelOn="默认展开"
            onCheckedChange={(checked) =>
              updateSettings({ defaultMetaExpanded: checked })
            }
          />
        </div>
        <div className="settings-page__item">
          <div className="settings-page__item-text">
            <span className="settings-page__item-name">折叠时标题行居中</span>
            <p className="settings-page__item-hint">
              卡片折叠时标题行水平居中；关闭则左对齐，与展开后的标题位
              置更连贯。
            </p>
          </div>
          <LabeledSwitch
            ariaLabel="折叠时标题行居中"
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
