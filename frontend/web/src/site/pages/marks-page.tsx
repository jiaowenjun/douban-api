import { useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { Navigate, useParams, useSearchParams } from 'react-router'

import { TextButton } from 'weimo-ui-core/components/text-button'

import { fetchDoubanMarks, fetchDoubanSubjectDims } from '../api'
import type { DoubanDims, DoubanMark } from '../api'
import { useDoubanUser } from '../douban-user'
import { filterIds } from '../facet-engine'
import { facetPathKey, facetValueLabel, parseFacetPath } from '../facet-model'
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

/** 标记列表页：读登录用户的缓存标记（GET /users/{uid}/marks），只读不触发抓取。
 *  分面筛选状态在 URL ?facet=dim=value——侧边栏筛选树写入、本页读取驱动
 *  请求；类别路由切换不带参数，天然清除筛选 */
export function MarksPage() {
  const { category } = useParams()
  const view = resolveMarksView(category)
  const { uid, profile, openLogin } = useDoubanUser()
  const [searchParams] = useSearchParams()

  // 请求序号：uid/类别切换或加载更多时自增，丢弃迟到的旧响应
  const seqRef = useRef(0)
  // 失败重试用的重放计数：自增即让下方 effect 重跑
  const [reloadNonce, setReloadNonce] = useState(0)

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
  // 筛选命中 id 集（前端 facet-engine 算）：数据源为当前类别的维度
  // 投影，随 uid/类别拉取缓存；无筛选为 undefined，筛选中数据未就绪
  // 为 null（列表保持 loading）
  const dimsKey = uid && realCategory ? `${uid}|${realCategory}` : null
  const [dimsSnapshot, setDimsSnapshot] = useState<{
    key: string | null
    data: DoubanDims | null
  }>({ key: null, data: null })
  if (dimsKey !== dimsSnapshot.key) {
    setDimsSnapshot({ key: dimsKey, data: null })
  }
  useEffect(() => {
    if (!dimsKey || !uid || !realCategory) return
    let cancelled = false
    fetchDoubanSubjectDims(uid, realCategory).then((data) => {
      if (!cancelled) setDimsSnapshot({ key: dimsKey, data })
    })
    return () => {
      cancelled = true
    }
  }, [dimsKey, uid, realCategory])

  const dimsReady =
    dimsKey !== null && dimsSnapshot.key === dimsKey && dimsSnapshot.data
  const matchedIds: string[] | null | undefined = !facetPath.length
    ? undefined
    : dimsReady && realCategory
      ? filterIds(realCategory, dimsSnapshot.data!.items, facetPath)
      : null
  const facetKey = facetPathKey(facetPath)
  const requestKey =
    uid && marksCategory && matchedIds !== null
      ? `${uid}|${marksCategory}|${facetKey}|${reloadNonce}`
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

  useEffect(() => {
    if (!requestKey || !uid || !marksCategory) return
    const seq = ++seqRef.current
    fetchDoubanMarks(
      uid,
      marksCategory,
      1,
      PAGE_SIZE,
      matchedIds?.slice(0, PAGE_SIZE),
    ).then((result) => {
      if (seq !== seqRef.current) return
      if (result.ok) {
        setSnapshot({
          key: requestKey,
          state: {
            phase: 'list',
            items: result.data.items,
            // ids 模式后端 total 是当页片数，命中总数以前端筛选结果为准
            total: matchedIds ? matchedIds.length : result.data.total,
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
    // matchedIds 为每次渲染的新数组引用，依赖它会造成请求死循环——
    // 以稳定的 facetKey 驱动（requestKey 已含），闭包取当次值
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [uid, marksCategory, requestKey, facetKey])

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
      matchedIds?.slice((nextPage - 1) * PAGE_SIZE, nextPage * PAGE_SIZE),
    )
    if (seq !== seqRef.current) return
    if (result.ok) {
      setSnapshot({
        key: requestKey,
        state: {
          phase: 'list',
          items: [...state.items, ...result.data.items],
          total: matchedIds ? matchedIds.length : result.data.total,
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
          {facetPath.length > 0 && realCategory
            ? ` · ${facetPath
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
      {content}
    </section>
  )
}
