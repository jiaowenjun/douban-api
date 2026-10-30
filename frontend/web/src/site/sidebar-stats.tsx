import { Heatmap } from 'weimo-ui-stats/components/heatmap'
import { StatGroup, type StatGroupItem } from 'weimo-ui-stats/components/stat-group'

import { useDoubanStatsQuery } from './douban-queries'
import { useDoubanUser } from './douban-user'
import type { DimCounts } from './facet-engine'
import { CATEGORY_NOUNS, MARKS_CATEGORIES } from './navigation'
import './sidebar-stats.css'

/** 侧边栏标记统计区：StatGroup 计数 + Heatmap 近 13 周热力图，布局
 *  对齐 weimo-biji 的 biji-side-bar__stats（header 下方紧跟 stats 区）。
 *  计数行：movie 按形态拆行（电影/电视剧——真一级，无「影视」聚合
 *  行；形态计数由侧边栏从 movieDims 本地聚合传入，未就绪/失败则
 *  movie 行暂缺，与树该分支不出现同策略），图书/音乐为类别行。
 *  未登录/加载失败整区静默隐藏；加载中给占位行。热力图暂仅展示
 *  ——站点还没有按日期过滤的页面，格子点击无动作。数据经
 *  react-query 缓存（key 带 uid 作用域），路由来回切零请求 */
export function SidebarStats({ movieKinds }: { movieKinds: DimCounts | null }) {
  const { uid } = useDoubanUser()
  const statsQuery = useDoubanStatsQuery(uid)

  if (!uid) return null
  // 请求失败（null）与未登录同策略：整区隐藏不打扰
  if (statsQuery.isSuccess && !statsQuery.data) return null

  const stats = statsQuery.data ?? null
  const statItems: StatGroupItem[] = [
    ...(movieKinds?.map(({ value, count }) => ({
      key: `movie/${value}`,
      value: String(count),
      label: value,
    })) ?? []),
    ...MARKS_CATEGORIES.filter((category) => category !== 'movie').map(
      (category) => ({
        key: category,
        value: String(stats?.totals[category] ?? 0),
        label: CATEGORY_NOUNS[category],
      }),
    ),
  ]

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
