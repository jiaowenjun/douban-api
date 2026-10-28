import type { DoubanMark } from './api'
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

/** 书影音标记卡片：封面 + 标题/评分/短评的列表条目（含 <li>），整卡链到豆瓣条目页 */
export function MarkCard({ mark }: MarkCardProps) {
  return (
    <li className="mark-card">
      <a
        className="mark-card__link"
        href={mark.subject_url}
        rel="noreferrer"
        target="_blank"
      >
        {mark.cover ? (
          <img
            alt=""
            className="mark-card__cover"
            loading="lazy"
            src={mark.cover}
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
          {mark.intro ? (
            <div className="mark-card__intro">{mark.intro}</div>
          ) : null}
          {mark.comment ? (
            <p className="mark-card__comment">{mark.comment}</p>
          ) : null}
          <div className="mark-card__date">{mark.date}</div>
        </div>
      </a>
    </li>
  )
}
