import { useEffect, useState } from 'react'
import { Heatmap } from 'weimo-ui-stats/components/heatmap'
import { StatGroup, type StatGroupItem } from 'weimo-ui-stats/components/stat-group'

import { fetchDoubanStats, type DoubanStats } from './api'
import { useDoubanUser } from './douban-user'
import { CATEGORY_NOUNS, MARKS_CATEGORIES } from './navigation'
import './sidebar-stats.css'

/** 统计数据快照：与 uid 绑定；uid 变化渲染期即回退 loading
 *  （模式同 marks-page 的 snapshot，避免 effect 内同步 setState） */
type StatsSnapshot = {
  uid: string | null
  phase: 'loading' | 'ready'
  stats: DoubanStats | null
}

/** 侧边栏标记统计区：StatGroup 三类计数（电影/图书/音乐）+ Heatmap
 *  近 13 周热力图，布局对齐 weimo-biji 的 biji-side-bar__stats（header
 *  下方紧跟 stats 区）。未登录/加载失败整区静默隐藏；加载中给占位行。
 *  热力图暂仅展示——站点还没有按日期过滤的页面，格子点击无动作 */
export function SidebarStats() {
  const { uid } = useDoubanUser()

  const [snapshot, setSnapshot] = useState<StatsSnapshot>({
    uid: null,
    phase: 'loading',
    stats: null,
  })
  if (uid !== snapshot.uid) {
    setSnapshot({ uid, phase: 'loading', stats: null })
  }

  useEffect(() => {
    if (!uid) return
    let cancelled = false
    fetchDoubanStats(uid).then((stats) => {
      if (!cancelled) {
        setSnapshot({ uid, phase: 'ready', stats })
      }
    })
    return () => {
      cancelled = true
    }
  }, [uid])

  if (!uid) return null
  // 请求失败（null）与未登录同策略：整区隐藏不打扰
  if (snapshot.phase === 'ready' && !snapshot.stats) return null

  const stats = snapshot.stats
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
