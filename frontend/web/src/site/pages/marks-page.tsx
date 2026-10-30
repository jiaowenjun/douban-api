import { useState } from 'react'
import type { ReactNode } from 'react'
import { Navigate, useParams, useSearchParams } from 'react-router'

import { TextButton } from 'weimo-ui-core/components/text-button'

import type { DoubanMark } from '../api'
import { useDoubanUser } from '../douban-user'
import {
  fetchDoubanMarksPage,
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
  resolveMarksView,
} from '../navigation'
import './marks-page.css'

const PAGE_SIZE = MARKS_PAGE_SIZE

/** RFC3339 → YYYY-MM-DD（cached_at 只取日期粒度即可） */
function formatDate(iso: string): string {
  return iso ? iso.slice(0, 10) : ''
}

/** 标记列表页：读登录用户的缓存标记（GET /users/{uid}/marks），只读不触发抓取。
 *  数据经 react-query 缓存（同 biji 模式）：首页查询与加载更多的每页
 *  各占独立 key，类别/筛选切回来命中缓存零请求；错误信封也进缓存，
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
    enabled: uid !== null && marksCategory !== undefined && matchedIds !== null,
  })

  // 加载更多累积页（page ≥ 2 的追加数据）：列表身份变化（uid/类别/
  // 筛选）时按 key 比对清空（render 期调整，模式同旧 snapshot）
  const listKey =
    uid && marksCategory ? `${uid}|${marksCategory}|${facetKey}` : null
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
  const first = marksQuery.data
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
    ? { phase: 'loading' }
    : !first.ok
      ? first.kind === 'not-cached'
        ? { phase: 'not-cached' }
        : { phase: 'error', message: first.message }
      : {
          phase: 'list',
          items: [...first.data.items, ...moreSnapshot.pages.flat()],
          // ids 模式后端 total 是当页片数，命中总数以前端筛选结果为准
          total: matchedIds ? matchedIds.length : first.data.total,
          loadedPages: 1 + moreSnapshot.pages.length,
          cachedAt: first.data.cached_at,
        }

  async function loadMore() {
    if (
      !uid ||
      !marksCategory ||
      state.phase !== 'list' ||
      moreSnapshot.loadingMore
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
            : `登录豆瓣账号后，这里会展示你${view.pageTitle}的记录。`}
        </p>
        <TextButton onClick={openLogin} type="button">
          登录豆瓣账号
        </TextButton>
      </div>
    )
  } else if (state.phase === 'loading') {
    content = <div className="marks-page__placeholder">加载中…</div>
  } else if (state.phase === 'not-cached') {
    content = (
      <div className="marks-page__placeholder">
        <p>用户 {profile?.username || uid} 的数据还没有同步过。</p>
        <p className="marks-page__hint">
          本站只读缓存，请先在服务端发起一次同步，完成后刷新本页即可看到
          {view.pageTitle}的记录。
        </p>
      </div>
    )
  } else if (state.phase === 'error') {
    content = (
      <div className="marks-page__placeholder">
        <p>加载失败：{state.message}</p>
        <TextButton
          onClick={() => {
            void marksQuery.refetch()
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
    // 形态与其他类别不提升。all 合并视图为「已标记 N 条书影音」
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
              disabled={moreSnapshot.loadingMore}
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
