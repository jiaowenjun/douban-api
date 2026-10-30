import { TextButton } from 'weimo-ui-core/components/text-button'
import { Heatmap } from 'weimo-ui-stats/components/heatmap'
import { StatGroup, type StatGroupItem } from 'weimo-ui-stats/components/stat-group'

import { useDoubanStatsQuery } from './douban-queries'
import { useDoubanUser } from './douban-user'
import { CATEGORY_NOUNS, MARKS_CATEGORIES } from './navigation'
import './sidebar-stats.css'

/** 侧边栏标记统计区：StatGroup 三类计数（影视/图书/音乐——movie 合并
 *  电影+电视剧计数显示「影视」，取 /stats 端点 totals）+ Heatmap 近
 *  13 周热力图，布局对齐 weimo-biji 的 biji-side-bar__stats（header
 *  下方紧跟 stats 区）。未登录整区隐藏；加载中占位行；失败且无旧
 *  数据（持久缓存/上一版本回退都没有）给轻量重试行——版本轮询自愈
 *  之外的立即出口。热力图暂仅展示——站点还没有按日期过滤的页面，
 *  格子点击无动作。数据经 react-query 缓存（版本进 query key），
 *  路由来回切零请求 */
export function SidebarStats() {
  const { uid } = useDoubanUser()
  const statsQuery = useDoubanStatsQuery(uid)

  if (!uid) return null
  // 成功必带数据（requireData 抛错语义），此分支仅防御
  if (statsQuery.isSuccess && !statsQuery.data) return null

  const stats = statsQuery.data ?? null
  const statItems: StatGroupItem[] = MARKS_CATEGORIES.map((category) => ({
    key: category,
    value: String(stats?.totals[category] ?? 0),
    label: CATEGORY_NOUNS[category],
  }))

  return (
    <section aria-label="标记统计" className="sidebar-stats">
      {statsQuery.isError && !stats ? (
        <div className="sidebar-stats__failed">
          <span>统计数据暂不可用</span>
          <TextButton
            onClick={() => void statsQuery.refetch()}
            type="button"
            variant="ghost"
          >
            重试
          </TextButton>
        </div>
      ) : stats ? (
        <>
          <StatGroup items={statItems} />
          <Heatmap dailyCounts={stats.daily_counts} />
        </>
      ) : (
        <p className="sidebar-stats__status" role="status">
          统计加载中…
        </p>
      )}
    </section>
  )
}
