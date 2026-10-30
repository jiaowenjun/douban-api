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

/** 五星评分：底色五星 + 按比例裁切的覆盖层（两段 ★★★★★ 叠加），
 *  value 为 0–5 的小数也精确支持（豆瓣 8.5 分 /2 = 4.25 星 → 85% 宽
 *  覆盖）。两种评分同一叠加裁切技术，填充色由传入根类名在 CSS 侧区
 *  分：我的评分 = 黑白灰收敛 15% 配方（随明暗主题翻转）、豆瓣评分 =
 *  元信息文字同色（--color-text-placeholder），仅凭位置与语境区分；
 *  与 mark-card-v2.tsx 的 StarRating 同源实现 */
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
 *  类别推导——看过/读过/听过。用户评分属「行为」，与日期动词同处顶栏 */
const CATEGORY_VERBS: Record<string, string> = {
  movie: '看过',
  book: '读过',
  music: '听过',
}

/** 条目元信息 → 单行摘要：影视「国家地区 / 类型 / 导演 / 主演」、图书
 *  「出版社 / 作者 / 译者」、音乐「类型 / 歌手」——不同元信息以「 / 」
 *  分隔，同一元信息内多值以空格分隔，不带「导演」「主演」等角色标签；
 *  物理规格如片长/页数不展示——卡片是列表条目不是详情页，超长由 CSS
 *  省略。intro 兜底源字段稀疏，逐项过滤，全空返回 null（由调用方回退
 *  intro 原文）。与 mark-card-v2.tsx 的 subjectMetaLine 同源 */
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

/** 书影音标记卡片，三段式一级结构：顶部标记行（日期 · 看过/读过/听
    过 · 我的评分星级 + 右侧元信息折叠开关）/ 可折叠条目信息区（封面
    左，文字列右——首行「标题 (年份) 别名」，第二行豆瓣评分「五星 +
    数字」（rating/2 换算星宽，无标签、aria-label 保留来源），第三行
    单行元信息摘要（「 / 」分隔元信息、空格分隔同域多值、无角色标
    签），无结构化数据回退 intro 原文；默认收起）/ 评论正文区
    （WdfRender 渲染标题 h1 + 短评段落），含 <li> 列表条目语义；材质
    （背景/边框/圆角/阴影）复用 weimo-ui-core 的 card-surface，与
    BaseCard 同源；折叠实现同源 BaseCard metaCollapsible；纯展示除折
    叠开关外无交互 */
export function MarkCard({ mark }: MarkCardProps) {
  const [metaExpanded, setMetaExpanded] = useState(false)
  // subject.cover 是 rexxar 中图（比收藏页小图清晰），两者同走本地 covers 代理
  const cover = mark.subject?.cover || mark.cover
  const metaLine = mark.subject ? subjectMetaLine(mark.subject) : null
  const year = mark.subject?.year
  const doubanRating = mark.subject?.rating.value
  // 标题行的可选段（年份/别名）全缺时整行不渲染——标题已由正文区 h1
  // 承载，孤行纯重复
  const hasOptionalTitleSegment = year != null || Boolean(mark.alt_title)

  return (
    <li className={getCardSurfaceClassName('mark-card')}>
      <div className="mark-card__header">
        {/* 日期·动词·星级必须同在一个 span 内：顶栏是 grid，散写的
            文本节点/元素会成为独立 grid 项被自动布局错位 */}
        <span>
          {mark.date} · {CATEGORY_VERBS[mark.category] ?? '标记'}
          {mark.my_rating ? (
            <>
              {' · '}
              <StarRating
                className="mark-card-rating"
                label={`我的评分：${mark.my_rating} 星`}
                value={mark.my_rating}
              />
            </>
          ) : null}
        </span>
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
          <div className="mark-card__info">
            {hasOptionalTitleSegment ? (
              <div className="mark-card__title-row">
                <bdi>{mark.title}</bdi>
                {year != null ? <span>({year})</span> : null}
                {/* bdi 隔离：阿文等 RTL 别名在行内自持方向、不扰动前后段 */}
                {mark.alt_title ? <bdi>{mark.alt_title}</bdi> : null}
              </div>
            ) : null}
            {doubanRating != null ? (
              <div className="mark-card__douban-rating">
                <StarRating
                  className="mark-card-douban-stars"
                  label={`豆瓣评分：${doubanRating.toFixed(1)} 分`}
                  value={doubanRating / 2}
                />
                {doubanRating.toFixed(1)}
              </div>
            ) : null}
            {metaLine ? (
              <div className="mark-card__meta">{metaLine}</div>
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
