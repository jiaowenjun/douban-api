import { useState } from 'react'
import { ChevronRight } from 'lucide-react'

import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'
import { GhostIconButton } from 'weimo-ui-core/components/ghost-icon-button'
import { WdfRender, type WdfDocument } from 'weimo-ui-editor/components/wdf-render'

import type { DoubanMark, DoubanSubject } from './api'
import './mark-card.css'

type MarkCardProps = {
  mark: DoubanMark
}

/** 评分星级：底色五星 + 按比例裁切的单色覆盖层（黑白灰随明暗主题翻转，
 *  两段 ★★★★★ 叠加）；配色见 css 中开关/滑块同源说明 */
function RatingStars({ value }: { value: number }) {
  const label = `我的评分：${value} 星`
  return (
    <span aria-label={label} className="mark-card-rating" title={label}>
      <span aria-hidden="true" className="mark-card-rating__base">
        ★★★★★
      </span>
      <span
        aria-hidden="true"
        className="mark-card-rating__fill"
        style={{ width: `${(value / 5) * 100}%` }}
      >
        ★★★★★
      </span>
    </span>
  )
}

/** 取数组前 n 个非空项，顿号连接 */
function names(list: string[] | undefined, n: number): string[] {
  return (list ?? []).filter((s) => s).slice(0, n)
}

function join(parts: string[]): string {
  return parts.filter(Boolean).join(' · ')
}

/** 条目元信息 → 首行"年份 · 国家/社 · 类型"（物理规格如片长/页数/介质
 *  不展示——对识别条目无用）+ 人名行数组（电影导演/主演各一行、图书
 *  作者/译者各一行、音乐歌手一行）。intro 兜底源字段稀疏，逐项过滤，
 *  全空返回 null（由调用方回退 intro 原文） */
function subjectMetaLines(
  subject: DoubanSubject,
): { primary: string; people: string[] } | null {
  let primary: string
  let people: string[]
  if (subject.category === 'movie') {
    const m = subject.movie
    primary = join([
      subject.year ? String(subject.year) : '',
      names(m?.countries, 2).join(' '),
      names(m?.genres, 3).join(' / '),
    ])
    const directors = names(m?.directors, 2)
    const actors = names(m?.actors, 2)
    people = [
      directors.length ? `导演 ${directors.join('、')}` : '',
      actors.length ? `主演 ${actors.join('、')}` : '',
    ].filter(Boolean)
  } else if (subject.category === 'book') {
    const b = subject.book
    primary = join([
      b?.pubdate?.[0] || (subject.year ? String(subject.year) : ''),
      names(b?.press, 1)[0] ?? '',
    ])
    const authors = names(b?.authors, 3)
    const translators = names(b?.translators, 2)
    people = [
      authors.length ? `作者 ${authors.join('、')}` : '',
      translators.length ? `译者 ${translators.join('、')}` : '',
    ].filter(Boolean)
  } else {
    const m = subject.music
    primary = join([
      m?.pubdate?.[0] || (subject.year ? String(subject.year) : ''),
      names(m?.genres, 2).join(' / '),
    ])
    const singers = names(m?.singers, 3)
    people = singers.length ? [`歌手 ${singers.join('、')}`] : []
  }
  return primary || people.length > 0 ? { primary, people } : null
}

/** 标题 + 短评 → WDF 文档：标题固定 h1（weimo 卡片体系的标题级——
 *  居中 650 字重，样式随包内 --md-* token 明暗自适应），短评作为正文
 *  段落跟在其后，无短评时文档仅含标题 */
function markDocument(mark: DoubanMark): WdfDocument {
  const content: WdfDocument['root']['content'] = [
    {
      type: 'heading',
      attrs: { level: 1 },
      content: [{ type: 'text', text: mark.title }],
    },
  ]
  if (mark.comment) {
    content.push({
      type: 'paragraph',
      content: [{ type: 'text', text: mark.comment }],
    })
  }
  return { schema: 'weimo.document', version: 1, root: { type: 'doc', content } }
}

/** 书影音标记卡片，三段式一级结构：顶部栏（标记日期 + 右侧元信息折叠
    开关）/ 条目信息区（封面左——正下方居中我的评分星级，文字列右——
    首行可选标题行「标题 · 可选标题 · 豆瓣评分」，其后年份国家类型与
    导演/主演各角色一行；可折叠，默认收起）/ 评论正文区（WdfRender 渲染
    标题 h1 + 短评段落），含 <li> 列表条目语义；材质（背景/边框/圆角/
    阴影）复用 weimo-ui-core 的 card-surface，与 BaseCard 同源；折叠实现
    同源 BaseCard metaCollapsible；纯展示除折叠开关外无交互 */
export function MarkCard({ mark }: MarkCardProps) {
  const [metaExpanded, setMetaExpanded] = useState(false)
  // subject.cover 是 rexxar 中图（比收藏页小图清晰），两者同走本地 covers 代理
  const cover = mark.subject?.cover || mark.cover
  const meta = mark.subject ? subjectMetaLines(mark.subject) : null
  const doubanRating = mark.subject?.rating.value
  // 可选标题行的可选段（可选标题/豆瓣评分）；全缺时整行不渲染——标题
  // 已由正文区 h1 承载，孤行纯重复
  const optionalTitleSegments = [
    mark.alt_title,
    doubanRating != null ? `${doubanRating.toFixed(1)}分` : '',
  ].filter(Boolean)
  const titleRow = [mark.title, ...optionalTitleSegments].join(' · ')

  return (
    <li className={getCardSurfaceClassName('mark-card')}>
      <div className="mark-card__header">
        <span>{mark.date}</span>
        <GhostIconButton
          aria-expanded={metaExpanded}
          aria-label={metaExpanded ? '收起元信息' : '展开元信息'}
          className="mark-card__meta-toggle"
          data-expanded={metaExpanded ? 'true' : undefined}
          onClick={() => setMetaExpanded((current) => !current)}
          size="sm"
          type="button"
        >
          <ChevronRight aria-hidden="true" />
        </GhostIconButton>
      </div>
      <div className="mark-card__meta-region" data-expanded={metaExpanded ? 'true' : 'false'}>
        <div className="mark-card__subject">
          <div className="mark-card__cover-region">
            {cover ? (
              <img
                alt=""
                className="mark-card__cover"
                loading="lazy"
                src={cover}
              />
            ) : (
              <div className="mark-card__cover mark-card__cover--empty" />
            )}
            {mark.my_rating ? <RatingStars value={mark.my_rating} /> : null}
          </div>
          <div className="mark-card__info">
            {optionalTitleSegments.length > 0 ? (
              <div className="mark-card__title-row">{titleRow}</div>
            ) : null}
            {meta ? (
              <>
                {meta.primary ? (
                  <div className="mark-card__meta">{meta.primary}</div>
                ) : null}
                {meta.people.map((line) => (
                  <div className="mark-card__meta" key={line}>
                    {line}
                  </div>
                ))}
              </>
            ) : mark.intro ? (
              <div className="mark-card__intro">{mark.intro}</div>
            ) : null}
          </div>
        </div>
      </div>
      <div className="mark-card__comment">
        <WdfRender content={markDocument(mark)} />
      </div>
    </li>
  )
}
