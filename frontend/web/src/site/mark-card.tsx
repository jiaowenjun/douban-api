import { getCardSurfaceClassName } from 'weimo-ui-core/components/card-surface'

import type { DoubanMark, DoubanSubject } from './api'
import './mark-card.css'

type MarkCardProps = {
  mark: DoubanMark
}

/** 评分星级：底色五星 + 按比例裁切的金色覆盖层（豆瓣式两段 ★★★★★ 叠加） */
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

/** 条目元信息 → 两行展示：首行"年份 · 国家/社 · 类型 · 片长/页数"，
 *  次行人名（导演/主演、作者/译者、歌手）。intro 兜底源字段稀疏，
 *  逐项过滤，全空返回 null（由调用方回退 intro 原文） */
function subjectMetaLines(
  subject: DoubanSubject,
): { primary: string; people: string } | null {
  let primary: string
  let people: string
  if (subject.category === 'movie') {
    const m = subject.movie
    primary = join([
      subject.year ? String(subject.year) : '',
      names(m?.countries, 2).join(' '),
      names(m?.genres, 3).join(' / '),
      names(m?.durations, 1)[0] ?? '',
    ])
    const directors = names(m?.directors, 2)
    const actors = names(m?.actors, 3)
    people = join([
      directors.length ? `导演 ${directors.join('、')}` : '',
      actors.length ? `主演 ${actors.join('、')}` : '',
    ])
  } else if (subject.category === 'book') {
    const b = subject.book
    primary = join([
      b?.pubdate?.[0] || (subject.year ? String(subject.year) : ''),
      names(b?.press, 1)[0] ?? '',
      b?.pages?.[0] ? `${b.pages[0]}页` : '',
    ])
    const authors = names(b?.authors, 3)
    const translators = names(b?.translators, 2)
    people = join([
      authors.length ? `作者 ${authors.join('、')}` : '',
      translators.length ? `译者 ${translators.join('、')}` : '',
    ])
  } else {
    const m = subject.music
    primary = join([
      m?.pubdate?.[0] || (subject.year ? String(subject.year) : ''),
      names(m?.genres, 2).join(' / '),
      names(m?.media, 1)[0] ?? '',
    ])
    const singers = names(m?.singers, 3)
    people = singers.length ? `歌手 ${singers.join('、')}` : ''
  }
  return primary || people ? { primary, people } : null
}

/** 书影音标记卡片：封面 + 标题/评分/条目元信息/短评的列表条目（含 <li>）；
    材质（背景/边框/圆角/阴影）复用 weimo-ui-core 的 card-surface，与 BaseCard 同源；
    纯展示，无链接与交互 */
export function MarkCard({ mark }: MarkCardProps) {
  // subject.cover 是 rexxar 中图（比收藏页小图清晰），两者同走本地 covers 代理
  const cover = mark.subject?.cover || mark.cover
  const meta = mark.subject ? subjectMetaLines(mark.subject) : null
  const doubanRating = mark.subject?.rating.value

  return (
    <li className={getCardSurfaceClassName('mark-card')}>
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
      <div className="mark-card__body">
        <div className="mark-card__title-row">
          <span className="mark-card__title">{mark.title}</span>
          {mark.my_rating ? <RatingStars value={mark.my_rating} /> : null}
        </div>
        {mark.alt_title ? (
          <div className="mark-card__alt">{mark.alt_title}</div>
        ) : null}
        {meta ? (
          <>
            <div className="mark-card__meta">{meta.primary}</div>
            {meta.people ? (
              <div className="mark-card__meta">{meta.people}</div>
            ) : null}
          </>
        ) : mark.intro ? (
          <div className="mark-card__intro">{mark.intro}</div>
        ) : null}
        {mark.comment ? (
          <p className="mark-card__comment">{mark.comment}</p>
        ) : null}
        <div className="mark-card__footer">
          <span>{mark.date}</span>
          {doubanRating != null ? (
            <span className="mark-card__db-rating">豆瓣 {doubanRating.toFixed(1)}</span>
          ) : null}
        </div>
      </div>
    </li>
  )
}
