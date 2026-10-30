import { useState } from 'react'
import { flushSync } from 'react-dom'
import { ChevronRight } from 'lucide-react'

import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { GhostIconButton } from 'weimo-ui-core/components/ghost-icon-button'
import { WdfRender, type WdfDocument } from 'weimo-ui-editor/components/wdf-render'

import type { DoubanMark, DoubanSubject } from './api'
import './mark-card-v4.css'

type MarkCardv4Props = {
  mark: DoubanMark
}

/** 五星评分：底色五星 + 按比例裁切的覆盖层（两段 ★★★★★ 叠加），
 *  value 为 0–5 的小数也精确支持（豆瓣 8.5 分 /2 = 4.25 星 → 85% 宽
 *  覆盖）。与 mark-card.tsx 的 StarRating 同源实现，配色由根类名在
 *  CSS 侧区分（我的评分收敛 15% / 豆瓣评分元信息同色） */
function StarRating({
  className,
  label,
  value,
}: {
  className: string
  label: string
  value: number
}) {
  return (
    <span aria-label={label} className={className} title={label}>
      <span aria-hidden="true" className={`${className}__base`}>
        ★★★★★
      </span>
      <span
        aria-hidden="true"
        className={`${className}__fill`}
        style={{ width: `${(value / 5) * 100}%` }}
      >
        ★★★★★
      </span>
    </span>
  )
}

/** 取数组前 n 个非空项 */
function names(list: string[] | undefined, n: number): string[] {
  return (list ?? []).filter((s) => s).slice(0, n)
}

/** 标记动词：看过/读过/听过（站点 status 恒 collect，按类别推导） */
const CATEGORY_VERBS: Record<string, string> = {
  movie: '看过',
  book: '读过',
  music: '听过',
}

/** 条目元信息 → 单行摘要（与 mark-card.tsx 的 subjectMetaLine 同源） */
function subjectMetaLine(subject: DoubanSubject): string | null {
  let parts: string[]
  if (subject.category === 'movie') {
    const m = subject.movie
    parts = [
      names(m?.countries, 2).join(' '),
      names(m?.genres, 3).join(' '),
      names(m?.directors, 2).join(' '),
      names(m?.actors, 2).join(' '),
    ]
  } else if (subject.category === 'book') {
    const b = subject.book
    parts = [
      names(b?.press, 1).join(' '),
      names(b?.authors, 3).join(' '),
      names(b?.translators, 2).join(' '),
    ]
  } else {
    const m = subject.music
    parts = [names(m?.genres, 2).join(' '), names(m?.singers, 3).join(' ')]
  }
  const line = parts.filter(Boolean).join(' / ')
  return line || null
}

/** 短评 → WDF 文档：仅正文段落（无标题），无短评返回 null */
function commentDocument(mark: DoubanMark): WdfDocument | null {
  if (!mark.comment) return null
  return {
    schema: 'weimo.document',
    version: 1,
    root: {
      type: 'doc',
      content: [
        {
          type: 'paragraph',
          content: [{ type: 'text', text: mark.comment }],
        },
      ],
    },
  }
}

/** VT 运行期标记类：挂在 html 根上，CSS 据此临时禁用折叠区/solo 的
 *  底层 CSS 过渡（见 mark-card-v4.css），避免底层动画在新快照层下播
 *  完后跳变 */
const VT_RUNNING_CLASS = 'mark-card-v4-vt'

type ViewTransitionApi = {
  startViewTransition?: (update: () => void) => { finished: Promise<void> }
}

/** MarkCardv4：MarkCard（v1）四段式结构 + 标题行共享元素过渡原型
    （View Transitions 路线，与 v3 的 FLIP 路线对照）。折叠开关切换经
    document.startViewTransition 包裹：回调内 flushSync 同步提交 React
    状态保证新快照含更新后 DOM；标题行的 view-transition-name 由 CSS
    只挂在「当前态副本」上（两副本同帧同名会让整个 transition 被
    skip），name 取每卡唯一的 --vt-title（comment_id 清洗）；折叠区与
    solo 的底层 200ms CSS 过渡在 VT 运行期被 html 标记类禁用，由快照
    动画全权接管（时长统一 200ms）。不支持 VT / prefers-reduced-motion
    时直接 setState 回退现有 CSS 行为。其余结构与样式与 v1 逐项同源
    （类名前缀 mark-card-v4__），见 mark-card.tsx 注释 */
export function MarkCardv4({ mark }: MarkCardv4Props) {
  const [metaExpanded, setMetaExpanded] = useState(false)
  const cover = mark.subject?.cover || mark.cover
  const metaLine = mark.subject ? subjectMetaLine(mark.subject) : null
  const year = mark.subject?.year
  const doubanRating = mark.subject?.rating.value
  const commentDoc = commentDocument(mark)
  const hasSubjectInfo =
    Boolean(cover) || doubanRating != null || Boolean(metaLine) || Boolean(mark.intro)

  const toggleMeta = () => {
    const next = !metaExpanded
    const doc = document as Document & ViewTransitionApi
    const reduced =
      window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
    if (typeof doc.startViewTransition !== 'function' || reduced) {
      setMetaExpanded(next)
      return
    }
    document.documentElement.classList.add(VT_RUNNING_CLASS)
    const transition = doc.startViewTransition(() => {
      // flushSync 强制 React 在回调返回前完成提交，新快照捕获到更新
      // 后的 DOM（否则快照仍为旧态、动画不触发）
      flushSync(() => {
        setMetaExpanded(next)
      })
    })
    transition.finished
      .catch(() => {
        /* skipTransition（如同名冲突）时 finished 拒绝，静默回退 */
      })
      .finally(() => {
        document.documentElement.classList.remove(VT_RUNNING_CLASS)
      })
  }

  const titleRowContent = (
    <>
      <bdi className="mark-card-v4__title">{mark.title}</bdi>
      {year != null ? <span>({year})</span> : null}
      {/* bdi 隔离：阿文等 RTL 别名在行内自持方向、不扰动前后段 */}
      {mark.alt_title ? <bdi>{mark.alt_title}</bdi> : null}
    </>
  )
  const renderTitleRow = (variant: 'solo' | 'inline') =>
    variant === 'solo' ? (
      <div className="mark-card-v4__title-solo">
        <div className="mark-card-v4__title-row">{titleRowContent}</div>
      </div>
    ) : (
      <div className="mark-card-v4__title-row">{titleRowContent}</div>
    )

  // view-transition-name 必须全文档唯一且为合法 ident：comment_id 清洗
  const vtTitleName = `mark-card-v4-title-${mark.comment_id.replace(/[^a-zA-Z0-9_-]/g, '-')}`

  return (
    <li
      className={getCardSurfaceClassName('mark-card-v4')}
      data-expanded={metaExpanded ? 'true' : 'false'}
      style={{ '--vt-title': vtTitleName } as React.CSSProperties}
    >
      <div className="mark-card-v4__header">
        {/* 日期·动词·星级必须同在一个 span 内：顶栏是 grid，散写的
            文本节点/元素会成为独立 grid 项被自动布局错位 */}
        <span>
          {mark.date} · {CATEGORY_VERBS[mark.category] ?? '标记'}
          {mark.my_rating ? (
            <>
              {' · '}
              <StarRating
                className="mark-card-v4-rating"
                label={`我的评分：${mark.my_rating} 星`}
                value={mark.my_rating}
              />
            </>
          ) : null}
        </span>
        {hasSubjectInfo ? (
          <GhostIconButton
            aria-expanded={metaExpanded}
            aria-label={metaExpanded ? '收起元信息' : '展开元信息'}
            className="mark-card-v4__meta-toggle"
            data-expanded={metaExpanded ? 'true' : undefined}
            onClick={toggleMeta}
            size="sm"
            type="button"
          >
            <ChevronRight aria-hidden="true" />
          </GhostIconButton>
        ) : null}
      </div>
      {renderTitleRow('solo')}
      {hasSubjectInfo ? (
        <div className="mark-card-v4__meta-region" data-expanded={metaExpanded ? 'true' : 'false'}>
          <div className="mark-card-v4__subject">
            {cover ? (
              <img
                alt=""
                className="mark-card-v4__cover"
                loading="lazy"
                src={cover}
              />
            ) : (
              <div className="mark-card-v4__cover mark-card-v4__cover--empty" />
            )}
            <div className="mark-card-v4__info">
              {renderTitleRow('inline')}
              {doubanRating != null ? (
                <div className="mark-card-v4__douban-rating">
                  <StarRating
                    className="mark-card-v4-douban-stars"
                    label={`豆瓣评分：${doubanRating.toFixed(1)} 分`}
                    value={doubanRating / 2}
                  />
                  {doubanRating.toFixed(1)}
                </div>
              ) : null}
              {metaLine ? (
                <div className="mark-card-v4__meta">{metaLine}</div>
              ) : mark.intro ? (
                <div className="mark-card-v4__intro">{mark.intro}</div>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
      {commentDoc ? (
        <div className="mark-card-v4__comment">
          <WdfRender content={commentDoc} />
        </div>
      ) : null}
    </li>
  )
}
