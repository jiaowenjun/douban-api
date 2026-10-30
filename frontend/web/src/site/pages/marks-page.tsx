import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { Navigate, useParams, useSearchParams } from 'react-router'

import { TextButton } from 'weimo-ui-core/components/text-button'

import type { DoubanMark } from '../api'
import { DoubanDataQueryError, triggerDoubanFirstSync } from '../api'
import { useDoubanUser } from '../douban-user'
import {
  doubanQueryKeys,
  fetchDoubanMarksPage,
  MarksQueryError,
  MARKS_PAGE_SIZE,
  useDoubanDimsQuery,
  useDoubanMarksPageQuery,
} from '../douban-queries'
import { filterIds } from '../facet-engine'
import { facetPathKey, facetValueLabel, parseFacetPath } from '../facet-model'
import { MarkCard } from '../mark-card'
import {
  CATEGORY_PAST_VERBS,
  CATEGORY_UNIT_NOUNS,
  DEFAULT_MARKS_PATH,
  moviePageTitle,
  parseRatingParam,
  ratingFilterLabel,
  resolveMarksView,
} from '../navigation'
import { queryClient } from '../../query-client'
import './marks-page.css'

const PAGE_SIZE = MARKS_PAGE_SIZE

/** 首发同步会话级去重（跨导航存活）：uid → 触发结果。重复触发本身
 *  无害（后端 single-flight 并入运行中任务），去重只为避免每次导航
 *  到未同步视图都发一次 POST；刷新页面后 Set 清空重触发亦安全 */
const firstSyncState = new Map<string, 'triggering' | 'syncing' | 'failed'>()

/** RFC3339 → YYYY-MM-DD（cached_at 只取日期粒度即可） */
function formatDate(iso: string): string {
  return iso ? iso.slice(0, 10) : ''
}

/** 标记列表页：读登录用户的缓存标记（GET /users/{uid}/marks），只读不触发抓取。
 *  数据经 react-query 缓存（同 biji 模式）：首页查询与加载更多的每页
 *  各占独立 key，类别/筛选切回来命中缓存零请求；失败保留已有成功数据，
 *  「重试」走 refetch 强制重发。分面筛选状态在 URL ?facet=dim=value
 *  ——侧边栏筛选树写入、本页读取驱动请求；类别路由切换不带参数，
 *  天然清除筛选 */
export function MarksPage() {
  const { category } = useParams()
  const view = resolveMarksView(category)
  const { uid, profile, openLogin } = useDoubanUser()
  const [searchParams] = useSearchParams()

  const marksCategory = view?.category
  // 分面筛选路径（可多步 AND：影视叶为 is_tv+genre 双条件，可清除）：
  // 解析自 URL——仅真实类别有维度白名单，all 合并视图与手改非法参数
  // 一律按无筛选处理（非法步起截断保留合法前缀，后端仍会兜底校验）
  const realCategory =
    marksCategory !== undefined && marksCategory !== 'all'
      ? marksCategory
      : undefined
  const facetPath = realCategory
    ? parseFacetPath(realCategory, searchParams.get('facet'))
    : []
  // 我的评分筛选（侧边栏「全部标记」子节点写入）：仅 all 合并视图有
  // 此参数，非法值按无筛选处理（后端仍会兜底校验）
  const rating =
    marksCategory === 'all' ? parseRatingParam(searchParams.get('rating')) : undefined
  // 占位文案展示名：movie 随形态（真一级，无「影视」聚合名；形态步由
  // App 的 MarksRoute 重定向保证，兜底电影），其余类别用页面标题
  const displayTitle =
    realCategory === 'movie'
      ? moviePageTitle(facetPath.find((s) => s.dim === 'is_tv')?.value)
      : (view?.pageTitle ?? '')

  // 维度投影与侧边栏共用缓存（同 key 去重）；筛选命中 id 集由前端
  // facet-engine 算——无筛选为 undefined，筛选中数据未就绪为 null
  // （列表保持 loading）
  const dimsQuery = useDoubanDimsQuery(uid, realCategory ?? null)
  const matchedIds: string[] | null | undefined = !facetPath.length
    ? undefined
    : dimsQuery.data && realCategory
      ? filterIds(realCategory, dimsQuery.data.items, facetPath)
      : null
  const facetKey = facetPathKey(facetPath)

  // 首页标记查询：enabled 保持旧门控（无筛选立即发；有筛选等 dims
  // 就绪算出命中集再发）；竞态防护由 react-query 内建（按 key 取消旧
  // 观察），不再需要手写 seqRef
  const marksQuery = useDoubanMarksPageQuery({
    uid,
    category: marksCategory ?? null,
    page: 1,
    ids: matchedIds?.slice(0, PAGE_SIZE),
    rating,
    enabled: uid !== null && marksCategory !== undefined && matchedIds !== null &&
      matchedIds?.length !== 0 && !dimsQuery.isPlaceholderData,
  })

  // 加载更多累积页（page ≥ 2 的追加数据）：列表身份变化（uid/类别/
  // 筛选/评分/版本）时按 key 比对清空（render 期调整，模式同旧 snapshot）
  const listKey =
    uid && marksCategory
      ? `${uid}|${marksCategory}|${facetKey}|${rating ?? ''}|${marksQuery.version}`
      : null
  const [moreSnapshot, setMoreSnapshot] = useState<{
    key: string | null
    pages: DoubanMark[][]
    loadingMore: boolean
    moreError: string | null
  }>({ key: null, pages: [], loadingMore: false, moreError: null })
  if (moreSnapshot.key !== listKey) {
    setMoreSnapshot({
      key: listKey,
      pages: [],
      loadingMore: false,
      moreError: null,
    })
  }

  // 列表态由「首页查询结果 + 追加页」派生，不再持有独立状态机
  const first = matchedIds?.length === 0
    ? { ok: true as const, data: { items: [], total: 0, cached_at: '' } }
    : marksQuery.data
  const state:
    | { phase: 'loading' }
    | { phase: 'not-cached' }
    | { phase: 'error'; message: string }
    | {
        phase: 'list'
        items: DoubanMark[]
        total: number
        loadedPages: number
        cachedAt: string
      } = !first
    ? marksQuery.error
      ? marksQuery.error instanceof MarksQueryError && marksQuery.error.kind === 'not-cached'
        ? { phase: 'not-cached' }
        : { phase: 'error', message: marksQuery.error.message }
      : dimsQuery.isError && matchedIds === null
        ? // 筛选态下 dims 未就绪的门控错误：404 未同步归入 not-cached
          // （首登自动同步对其生效——默认落地 /movie 必带形态筛选步，
          // 未同步用户先撞到的就是这条路），其余才算加载失败
          dimsQuery.error instanceof DoubanDataQueryError &&
            dimsQuery.error.kind === 'not-cached'
          ? { phase: 'not-cached' }
          : { phase: 'error', message: dimsQuery.error.message }
        : { phase: 'loading' }
    : {
          phase: 'list',
          items: [...first.data.items, ...moreSnapshot.pages.flat()],
          // ids 模式后端 total 是当页片数，命中总数以前端筛选结果为准
          total: matchedIds ? matchedIds.length : first.data.total,
          loadedPages: 1 + moreSnapshot.pages.length,
          cachedAt: first.data.cached_at,
        }

  // 首发同步：数据未同步（not-cached）时自动触发一次自助全量同步，
  // 成功后展示同步中引导；同步发布时 revision bump → 版本查询感知 →
  // 版本 key 变化自动重拉，数据无需手动刷新即出现。展示态 render 期
  // 直读会话级 Map（uid 天然隔离，无切号重置），异步回填后 tick 触发
  // 重渲染——effect 内不同步 setState（React Compiler 会因此放弃优化
  // 整个组件）；触发失败本会话不自动重试（刷新页面重走一次）
  const [, setFirstSyncTick] = useState(0)
  const syncPhase = uid ? firstSyncState.get(uid) : undefined
  useEffect(() => {
    if (state.phase !== 'not-cached' || !uid) return
    if (firstSyncState.has(uid)) return
    firstSyncState.set(uid, 'triggering')
    void triggerDoubanFirstSync(uid).then((task) => {
      firstSyncState.set(uid, task ? 'syncing' : 'failed')
      setFirstSyncTick((tick) => tick + 1)
    })
  }, [state.phase, uid])

  // 同步中每 10s 失效版本查询：发布瞬间尽快感知 revision bump（60s
  // 常规轮询兜底），版本变化触发全量查询按新 key 自动重拉
  useEffect(() => {
    if (state.phase !== 'not-cached' || syncPhase !== 'syncing' || !uid) {
      return
    }
    const timer = setInterval(() => {
      void queryClient.invalidateQueries({
        queryKey: [...doubanQueryKeys.uid(uid), 'versions'],
      })
    }, 10_000)
    return () => clearInterval(timer)
  }, [state.phase, syncPhase, uid])

  async function loadMore() {
    if (
      !uid ||
      !marksCategory ||
      state.phase !== 'list' ||
      moreSnapshot.loadingMore ||
      marksQuery.isPlaceholderData
    ) {
      return
    }
    const capturedKey = listKey
    const nextPage = state.loadedPages + 1
    setMoreSnapshot((s) =>
      s.key === capturedKey ? { ...s, loadingMore: true, moreError: null } : s,
    )
    const result = await fetchDoubanMarksPage({
      uid,
      category: marksCategory,
      page: nextPage,
      ids: matchedIds?.slice((nextPage - 1) * PAGE_SIZE, nextPage * PAGE_SIZE),
      rating,
      version: marksQuery.version,
    })
    setMoreSnapshot((s) => {
      // 期间类别/筛选已切换：丢弃迟到页（对齐旧 seqRef 竞态防护）
      if (s.key !== capturedKey) return s
      if (result.ok) {
        return {
          ...s,
          pages: [...s.pages, result.data.items],
          loadingMore: false,
          moreError: null,
        }
      }
      return { ...s, loadingMore: false, moreError: result.message }
    })
  }

  if (!view) {
    return <Navigate replace to={DEFAULT_MARKS_PATH} />
  }

  let content: ReactNode
  if (!uid) {
    content = (
      <div className="marks-page__placeholder">
        <p>
          {view.category === 'all'
            ? '登录豆瓣账号后，这里会展示你的全部书影音标记。'
            : `登录豆瓣账号后，这里会展示你${displayTitle}的记录。`}
        </p>
        <TextButton onClick={openLogin} type="button">
          登录豆瓣账号
        </TextButton>
      </div>
    )
  } else if (state.phase === 'loading') {
    content = <div className="marks-page__placeholder">加载中…</div>
  } else if (state.phase === 'not-cached') {
    // 首发同步三档展示：触发中/同步中给自动同步引导（完成后自动翻页），
    // 触发失败回落手动指引
    content =
      syncPhase === 'triggering' || syncPhase === 'syncing' ? (
        <div className="marks-page__placeholder">
          <p>
            首次使用：正在从豆瓣同步 {profile?.username || uid} 的标记。
          </p>
          <p className="marks-page__hint">
            全量抓取通常需要几分钟，完成后这里会自动展示，无需手动刷新。
          </p>
        </div>
      ) : (
        <div className="marks-page__placeholder">
          <p>用户 {profile?.username || uid} 的数据还没有同步过。</p>
          <p className="marks-page__hint">
            {syncPhase === 'failed'
              ? '自动同步发起失败（服务不可达或请求被限流），请稍后刷新页面重试，或在管理端手动发起同步。'
              : `本站只读缓存，请先在服务端发起一次同步，完成后刷新本页即可看到${displayTitle}的记录。`}
          </p>
        </div>
      )
  } else if (state.phase === 'error') {
    content = (
      <div className="marks-page__placeholder">
        <p>加载失败：{state.message}</p>
        <TextButton
          onClick={() => {
            if (matchedIds === null || dimsQuery.isPlaceholderData) void dimsQuery.refetch()
            else void marksQuery.refetch()
          }}
          type="button"
          variant="ghost"
        >
          重试
        </TextButton>
      </div>
    )
  } else {
    const hasMore = state.items.length < state.total
    const syncedDate = formatDate(state.cachedAt)
    // meta 行名词：影视筛了电影/电视剧形态时提升为该形态（…部电影/
    // …部电视剧），该步从后缀去除避免「部电影 · 电影」重复；未分类
    // 形态与其他类别不提升。all 合并视图为「已标记 N 条书影音」，评分
    // 筛选时后缀追加「· 我的N星标记」
    const kindStep =
      realCategory === 'movie'
        ? facetPath.find((step) => step.dim === 'is_tv')
        : undefined
    const kindNoun =
      kindStep && (kindStep.value === '电影' || kindStep.value === '电视剧')
        ? `部${kindStep.value}`
        : null
    const suffixPath = kindNoun
      ? facetPath.filter((step) => step.dim !== 'is_tv')
      : facetPath
    content = (
      <>
        <p className="marks-page__meta">
          {profile?.username || uid}
          {realCategory
            ? ` ${CATEGORY_PAST_VERBS[realCategory]} ${state.total} ${kindNoun ?? CATEGORY_UNIT_NOUNS[realCategory]}`
            : ` 已标记 ${state.total} 条书影音`}
          {suffixPath.length > 0 && realCategory
            ? ` · ${suffixPath
                .map((step) => facetValueLabel(realCategory, step.dim, step.value))
                .join(' / ')}`
            : ''}
          {rating ? ` · ${ratingFilterLabel(rating)}` : ''}
          {syncedDate ? ` · 同步于 ${syncedDate}` : ''}
        </p>
        <ul className="marks-list">
          {state.items.map((mark) => (
            <MarkCard key={mark.comment_id || mark.subject_id} mark={mark} />
          ))}
        </ul>
        {hasMore ? (
          <div className="marks-page__more">
            {moreSnapshot.moreError ? (
              <span className="marks-page__more-error">
                {moreSnapshot.moreError}
              </span>
            ) : null}
            <TextButton
              disabled={moreSnapshot.loadingMore || marksQuery.isPlaceholderData}
              onClick={loadMore}
              type="button"
            >
              {moreSnapshot.loadingMore
                ? '加载中…'
                : `加载更多（${state.items.length}/${state.total}）`}
            </TextButton>
          </div>
        ) : null}
      </>
    )
  }

  return (
    <section className="marks-page">
      {content}
    </section>
  )
}
