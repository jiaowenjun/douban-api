import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { LabeledSwitch } from 'weimo-ui-core/components/labeled-switch'

import { useSiteSettings } from '../site-settings'
import './settings-page.css'

/** 设置页：站点本地偏好（localStorage 持久化，见 site-settings）。
 *  当前一组「卡片显示」——标记卡片条目元信息区的初始展开态；设置仅
 *  影响此后挂载的卡片（返回列表页整体重挂载即全量生效），不打扰已
 *  挂载卡片的手动展开。后续设置项在此页按分组追加 */
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
      </div>
    </section>
  )
}
