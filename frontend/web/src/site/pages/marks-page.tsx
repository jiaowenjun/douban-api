import { useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { Navigate, useParams } from 'react-router'

import { TextButton } from 'weimo-ui-core/components/text-button'

import { fetchDoubanFacets, fetchDoubanMarks } from '../api'
import type { DoubanFacets, DoubanMark } from '../api'
import { useDoubanUser } from '../douban-user'
import { FacetTree } from '../facet-tree'
import { facetDimLabel } from '../facet-model'
import type { FacetSelection } from '../facet-model'
import { MarkCard } from '../mark-card'
import { DEFAULT_MARKS_PATH, resolveMarksView } from '../navigation'
import './marks-page.css'

const PAGE_SIZE = 20

/** 列表态；加载更多失败保留已加载条目，只在按钮旁提示 */
type MarksState =
  | { phase: 'loading' }
  | { phase: 'not-cached' }
  | { phase: 'error'; message: string }
  | {
      phase: 'list'
      items: DoubanMark[]
      total: number
      page: number
      cachedAt: string
      loadingMore: boolean
      moreError: string | null
    }

/** RFC3339 → YYYY-MM-DD（cached_at 只取日期粒度即可） */
function formatDate(iso: string): string {
  return iso ? iso.slice(0, 10) : ''
}

/** 标记列表页：读登录用户的缓存标记（GET /users/{uid}/marks），只读不触发抓取 */
export function MarksPage() {
  const { category } = useParams()
  const view = resolveMarksView(category)
  const { uid, profile, openLogin } = useDoubanUser()

  // 请求序号：uid/类别切换或加载更多时自增，丢弃迟到的旧响应
  const seqRef = useRef(0)
  // 失败重试用的重放计数：自增即让下方 effect 重跑
  const [reloadNonce, setReloadNonce] = useState(0)

  const marksCategory = view?.category
  // 分面筛选（单选，可清除）；类别切换时在渲染期同步清除，
  // 避免旧类别的维度值打到新类别上（后端按类别白名单校验会 400）
  const [facet, setFacet] = useState<FacetSelection | null>(null)
  const [facetCategory, setFacetCategory] = useState(marksCategory)
  if (marksCategory !== facetCategory) {
    setFacetCategory(marksCategory)
    setFacet(null)
  }
  const facetParam = facet ? `${facet.dim}=${facet.value}` : null
  const requestKey =
    uid && marksCategory
      ? `${uid}|${marksCategory}|${facetParam ?? ''}|${reloadNonce}`
      : null

  // 状态与它对应的请求 key 绑定；渲染期发现 key 变化即回到 loading
  // （render 期调整，模式同 login-dialog 的 prevOpen，避免 effect 内同步 setState）
  const [snapshot, setSnapshot] = useState<{
    key: string | null
    state: MarksState
  }>({ key: null, state: { phase: 'loading' } })
  if (requestKey !== snapshot.key) {
    setSnapshot({ key: requestKey, state: { phase: 'loading' } })
  }
  const state = snapshot.state

  // 分面聚合：仅随 uid/类别变化重取，与筛选选择无关；
  // 失败/未同步返回 null，面板静默隐藏不打断列表
  const facetsKey = uid && marksCategory ? `${uid}|${marksCategory}` : null
  const [facetsSnapshot, setFacetsSnapshot] = useState<{
    key: string | null
    data: DoubanFacets | null
  }>({ key: null, data: null })
  if (facetsKey !== facetsSnapshot.key) {
    setFacetsSnapshot({ key: facetsKey, data: null })
  }
  useEffect(() => {
    if (!facetsKey || !uid || !marksCategory) return
    let cancelled = false
    fetchDoubanFacets(uid, marksCategory).then((data) => {
      if (!cancelled) {
        setFacetsSnapshot({ key: facetsKey, data })
      }
    })
    return () => {
      cancelled = true
    }
  }, [facetsKey, uid, marksCategory])

  useEffect(() => {
    if (!requestKey || !uid || !marksCategory) return
    const seq = ++seqRef.current
    fetchDoubanMarks(uid, marksCategory, 1, PAGE_SIZE, facetParam).then(
      (result) => {
        if (seq !== seqRef.current) return
        if (result.ok) {
          setSnapshot({
            key: requestKey,
            state: {
              phase: 'list',
              items: result.data.items,
              total: result.data.total,
              page: result.data.page,
              cachedAt: result.data.cached_at,
              loadingMore: false,
              moreError: null,
            },
          })
        } else if (result.kind === 'not-cached') {
          setSnapshot({ key: requestKey, state: { phase: 'not-cached' } })
        } else {
          setSnapshot({
            key: requestKey,
            state: { phase: 'error', message: result.message },
          })
        }
      },
    )
  }, [uid, marksCategory, requestKey, facetParam])

  async function loadMore() {
    if (!uid || !marksCategory || state.phase !== 'list' || state.loadingMore) {
      return
    }
    const seq = ++seqRef.current
    setSnapshot({
      key: requestKey,
      state: { ...state, loadingMore: true, moreError: null },
    })
    const nextPage = state.page + 1
    const result = await fetchDoubanMarks(
      uid,
      marksCategory,
      nextPage,
      PAGE_SIZE,
      facetParam,
    )
    if (seq !== seqRef.current) return
    if (result.ok) {
      setSnapshot({
        key: requestKey,
        state: {
          phase: 'list',
          items: [...state.items, ...result.data.items],
          total: result.data.total,
          page: result.data.page,
          cachedAt: result.data.cached_at,
          loadingMore: false,
          moreError: null,
        },
      })
    } else {
      setSnapshot({
        key: requestKey,
        state: { ...state, loadingMore: false, moreError: result.message },
      })
    }
  }

  if (!view) {
    return <Navigate replace to={DEFAULT_MARKS_PATH} />
  }

  // 分面筛选面板：聚合数据就绪才出现；标题行右侧给当前筛选的清除入口
  const facetsData = facetsSnapshot.data
  const facetPanel =
    uid && marksCategory && facetsData ? (
      <section className="marks-page__facet">
        <div className="marks-page__facet-head">
          <span className="marks-page__facet-title">筛选</span>
          {facet ? (
            <TextButton
              onClick={() => setFacet(null)}
              type="button"
              variant="ghost"
            >
              清除筛选（{facetDimLabel(marksCategory, facet.dim)}：{facet.value}）
            </TextButton>
          ) : null}
        </div>
        <FacetTree
          category={marksCategory}
          facets={facetsData}
          onSelect={setFacet}
          selection={facet}
        />
      </section>
    ) : null

  let content: ReactNode
  if (!uid) {
    content = (
      <div className="marks-page__placeholder">
        <p>登录豆瓣账号后，这里会展示你{view.pageTitle}的记录。</p>
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
          onClick={() => setReloadNonce((n) => n + 1)}
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
    content = (
      <>
        <p className="marks-page__meta">
          {profile?.username || uid} 共 {state.total} 条
          {facet
            ? ` · ${facetDimLabel(view.category, facet.dim)}：${facet.value}`
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
            {state.moreError ? (
              <span className="marks-page__more-error">
                {state.moreError}
              </span>
            ) : null}
            <TextButton
              disabled={state.loadingMore}
              onClick={loadMore}
              type="button"
            >
              {state.loadingMore
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
      <h1 className="marks-page__title">{view.pageTitle}</h1>
      {facetPanel}
      {content}
    </section>
  )
}
