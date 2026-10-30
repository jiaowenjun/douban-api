import { Heatmap } from 'weimo-ui-stats/components/heatmap'
import { StatGroup, type StatGroupItem } from 'weimo-ui-stats/components/stat-group'

import { useDoubanStatsQuery } from './douban-queries'
import { useDoubanUser } from './douban-user'
import { CATEGORY_NOUNS, MARKS_CATEGORIES } from './navigation'
import './sidebar-stats.css'

/** 侧边栏标记统计区：StatGroup 三类计数（影视/图书/音乐——movie 合并
 *  电影+电视剧计数显示「影视」，取 /stats 端点 totals）+ Heatmap 近
 *  13 周热力图，布局对齐 weimo-biji 的 biji-side-bar__stats（header
 *  下方紧跟 stats 区）。未登录/加载失败整区静默隐藏；加载中给占位行。
 *  热力图暂仅展示——站点还没有按日期过滤的页面，格子点击无动作。
 *  数据经 react-query 缓存（key 带 uid 作用域），路由来回切零请求 */
export function SidebarStats() {
  const { uid } = useDoubanUser()
  const statsQuery = useDoubanStatsQuery(uid)

  if (!uid) return null
  // 请求失败（null）与未登录同策略：整区隐藏不打扰
  if ((statsQuery.isSuccess || statsQuery.isError) && !statsQuery.data) return null

  const stats = statsQuery.data ?? null
  const statItems: StatGroupItem[] = MARKS_CATEGORIES.map((category) => ({
    key: category,
    value: String(stats?.totals[category] ?? 0),
    label: CATEGORY_NOUNS[category],
  }))

  return (
    <section aria-label="标记统计" className="sidebar-stats">
      {stats ? (
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
