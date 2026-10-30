import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { WdfRender, type WdfDocument } from 'weimo-ui-editor/components/wdf-render'

import type { DoubanMark, DoubanSubject } from './api'
import './mark-card-v2.css'

type MarkCardv2Props = {
  mark: DoubanMark
}

/** 五星评分：底色五星 + 按比例裁切的覆盖层（两段 ★★★★★ 叠加），
 *  value 为 0–5 的小数也精确支持（豆瓣 8.5 分 /2 = 4.25 星 → 85% 宽
 *  覆盖）。两种评分同一叠加裁切技术，配色统一收敛灰（黑白灰随明暗主
 *  题翻转），仅凭位置与语境区分。与 mark-card.tsx 的实现同源——v1
 *  冻结作对照、v2 自带一份，对比期结束后随胜出版本归并 */
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

/** 标记动词：站点当前只展示 collect（MARKS_STATUS 恒为 'collect'），按
 *  类别推导——看过/读过/听过。用户评分属「行为」，与日期动词同处标记行 */
const CATEGORY_VERBS: Record<string, string> = {
  movie: '看过',
  book: '读过',
  music: '听过',
}

/** 条目元信息 → 单行摘要：影视「国家地区 / 类型 / 导演 / 主演」、图书
 *  「出版社 / 作者 / 译者」、音乐「类型 / 歌手」——不同元信息以「 / 」
 *  分隔，同一元信息内多值以空格分隔，不带「导演」「主演」等角色标签；
 *  物理规格如片长/页数不展示——列表卡是单行摘要不是详情页，超长由
 *  CSS 省略。intro 兜底源字段稀疏，逐项过滤，全空返回 null（由调用
 *  方回退 intro 原文） */
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

/** 短评 → WDF 文档：仅正文段落。标题由身份区承载、不再嵌 h1——v1 的
 *  「元信息标题行 + 正文 h1」双标题在身份区常驻后即为纯重复；无短评
 *  返回 null（正文区整区不渲染） */
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

/** 书影音标记卡片 v2（与 mark-card.tsx 的 v1 并存对比，生产列表页仍用
    v1）。三区结构对齐豆瓣「条目身份 → 豆瓣评价 → 用户标记」的信息架构，
    全部常驻展示、纯展示零交互（无折叠）：
    - 标记行：日期 · 看过/读过/听过 · 我的评分星级——左对齐单行，未打分
      止于动词；
    - 身份区：封面 64px + 标题行（标题唯一出现处 + 年份 + 别名，后两
      者降一级弱灰 sm）+ 豆瓣评分独占一行（五星 + 数字，rating/2 换算
      五星宽度（8.5 分 → 4.25 星），弱灰 sm 与元信息同级，aria-label
      保留评分来源）+ 单行元信息摘要（国家地区 / 类型 / 演职员等以斜
      杠分隔、无角色标签，超长省略；无结构化数据回退 intro 原文）；
    - 正文区：仅短评段落，无短评整区不渲染。
    材质复用 card-surface；含 <li> 列表条目语义 */
export function MarkCardv2({ mark }: MarkCardv2Props) {
  // subject.cover 是 rexxar 中图（比收藏页小图清晰），两者同走本地 covers 代理
  const cover = mark.subject?.cover || mark.cover
  const metaLine = mark.subject ? subjectMetaLine(mark.subject) : null
  const year = mark.subject?.year
  const doubanRating = mark.subject?.rating.value
  const altTitle = mark.alt_title
  const commentDoc = commentDocument(mark)

  return (
    <li className={getCardSurfaceClassName('mark-card-v2')}>
      <div className="mark-card-v2__header">
        <span>
          {mark.date} · {CATEGORY_VERBS[mark.category] ?? '标记'}
        </span>
        {mark.my_rating ? (
          <>
            {' · '}
            <StarRating
              className="mark-card-v2-rating"
              label={`我的评分：${mark.my_rating} 星`}
              value={mark.my_rating}
            />
          </>
        ) : null}
      </div>
      <div className="mark-card-v2__subject">
        {cover ? (
          <img
            alt={mark.title}
            className="mark-card-v2__cover"
            loading="lazy"
            src={cover}
          />
        ) : (
          <div className="mark-card-v2__cover mark-card-v2__cover--empty" />
        )}
        <div className="mark-card-v2__info">
          <div className="mark-card-v2__title-row">
            <bdi className="mark-card-v2__title">{mark.title}</bdi>
            {year != null ? (
              <span className="mark-card-v2__year">({year})</span>
            ) : null}
            {/* bdi 隔离：阿文等 RTL 别名在标题行内自持方向、不扰动前后段 */}
            {altTitle ? <bdi className="mark-card-v2__alt">{altTitle}</bdi> : null}
          </div>
          {doubanRating != null ? (
            <div className="mark-card-v2__douban-rating">
              <StarRating
                className="mark-card-v2-douban-stars"
                label={`豆瓣评分：${doubanRating.toFixed(1)} 分`}
                value={doubanRating / 2}
              />
              {doubanRating.toFixed(1)}
            </div>
          ) : null}
          {metaLine ? (
            <div className="mark-card-v2__meta">{metaLine}</div>
          ) : mark.intro ? (
            <div className="mark-card-v2__intro">{mark.intro}</div>
          ) : null}
        </div>
      </div>
      {commentDoc ? (
        <div className="mark-card-v2__comment">
          <WdfRender content={commentDoc} />
        </div>
      ) : null}
    </li>
  )
}
