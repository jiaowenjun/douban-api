import { Link } from 'react-router'

import type { DoubanMark } from '../api'
import { MarkCard } from '../mark-card'
import './mark-card-demo.css'

/** 演示页路由（HashRouter 下即 #/demo/mark-card）；纯 mock 数据，不请求后端 */
export const DEMO_MARK_CARD_PATH = '/demo/mark-card'

/** 演示封面：取自后端 covers 代理的真实地址，便于核对封面图样式 */
const COVER_A =
  '/api/v1/covers?u=https%3A%2F%2Fimg9.doubanio.com%2Fview%2Fphoto%2Fs_ratio_poster%2Fpublic%2Fp2935834896.webp'
const COVER_B =
  '/api/v1/covers?u=https%3A%2F%2Fimg9.doubanio.com%2Fview%2Fphoto%2Fs_ratio_poster%2Fpublic%2Fp2885402364.webp'
const COVER_C =
  '/api/v1/covers?u=https%3A%2F%2Fimg3.doubanio.com%2Fview%2Fphoto%2Fs_ratio_poster%2Fpublic%2Fp2574853860.webp'

const LONG_TITLE =
  '超级无敌旋风加长导演剪辑终极纪念收藏限量展映完整版电影标题示例'
const LONG_INTRO =
  '2026-10-28(中国大陆) / 示例导演 / 示例演员甲 / 示例演员乙 / 示例演员丙 / 中国大陆 / 120分钟 / 剧情 / 喜剧 / 爱情 / 汉语普通话 / 粤语 / 这一行简介非常长，用来验证完整展示时能否自然换行、不与封面重叠'
const LONG_COMMENT =
  '这条短评特意写得很长很长，用来验证短评区域完整展示、卡片随内容自动撑开高度的排版规则。第一行交代剧情观感，第二行继续铺陈导演技法与视听语言的分析，第三行收束到个人化的情绪与结论，后面还可以有第四行、第五行甚至更多——无论多长都应原样显示，不留省略号，卡片高度由正文决定。'

/** 基准字段：全部可选字段留空，各场景按需覆盖 */
const BASE_MARK: DoubanMark = {
  category: 'movie',
  status: 'collect',
  title: '示例电影',
  alt_title: '',
  my_rating: null,
  date: '2026-10-28',
  comment: '',
  subject_url: 'https://movie.douban.com/subject/37322586/',
  subject_id: '0',
  cover: '',
  comment_id: 'demo-0',
  intro: '',
}

function mark(overrides: Partial<DoubanMark>): DoubanMark {
  return { ...BASE_MARK, ...overrides }
}

const FULL_MARK = mark({
  comment_id: 'demo-full',
  title: '完整字段示例',
  alt_title: 'A Full-Featured Example',
  my_rating: 5,
  cover: COVER_A,
  intro:
    '2026-10-28(中国大陆) / 示例导演 / 示例演员 / 中国大陆 / 120分钟 / 剧情 / 喜剧 / 汉语普通话',
  comment:
    '这是一条完整的短评，用于展示所有字段齐备时的卡片形态：顶部为标记日期，展开元信息首行为「标题 · 可选标题 · 豆瓣评分」，其后年份国家类型一行、导演与主演各一行，下方为完整展示的短评。',
  subject: {
    category: 'movie',
    subject_id: 'demo-subject-full',
    title: '完整字段示例',
    original_title: 'A Full-Featured Example',
    aka: [],
    year: 2026,
    rating: { value: 8.5, count: 123456 },
    cover: '',
    url: 'https://movie.douban.com/subject/0/',
    status: 'collect',
    fetched_at: '2026-10-28T00:00:00Z',
    source: 'rexxar',
    movie: {
      directors: ['示例导演'],
      actors: ['示例演员甲', '示例演员乙'],
      genres: ['剧情', '喜剧'],
      countries: ['中国大陆'],
      languages: ['汉语普通话'],
      durations: ['120分钟'],
      pubdate: ['2026-10-28'],
      is_tv: false,
      episodes_count: null,
    },
  },
})

/** 可选标题为阿拉伯文（RTL）的回归用例：标题行各段 <bdi> 隔离前，
 *  bidi 重排会把 RTL 段之后的「8.9分」拆到行两端（「生日快乐」卡实测） */
const RTL_MARK = mark({
  comment_id: 'demo-rtl-alt',
  title: '阿文别名示例',
  alt_title: 'هابي بيرث داي',
  my_rating: 4,
  cover: COVER_B,
  intro: '阿文可选标题与豆瓣评分混排时，各段应保持顺序、评分不被拆散。',
  subject: {
    category: 'movie',
    subject_id: 'demo-subject-rtl',
    title: '阿文别名示例',
    original_title: 'هابي بيرث داي',
    aka: [],
    year: null,
    rating: { value: 8.9, count: null },
    cover: '',
    url: 'https://movie.douban.com/subject/0/',
    status: 'collect',
    fetched_at: '2026-10-29T00:00:00Z',
    source: 'rexxar',
  },
})

const RATING_MARKS = [1, 2, 3, 4, 5].map((n) =>
  mark({
    comment_id: `demo-rating-${n}`,
    title: `评分 ${n} 星`,
    my_rating: n,
    cover: COVER_B,
  }),
)

const MISSING_FIELD_MARKS = [
  mark({
    comment_id: 'demo-no-rating',
    title: '无评分',
    cover: COVER_C,
    intro: '未打分时标题行右侧不出现星级。',
  }),
  mark({
    comment_id: 'demo-no-alt',
    title: '无又名',
    my_rating: 4,
    cover: COVER_A,
    intro: '外文名缺失时直接进入简介行。',
  }),
  mark({
    comment_id: 'demo-no-intro',
    title: '无简介',
    my_rating: 3,
    cover: COVER_B,
    comment: '演职员等简介缺失时直接显示短评。',
  }),
  mark({
    comment_id: 'demo-no-comment',
    title: '无短评',
    my_rating: 2,
    cover: COVER_C,
    intro: '只标记不评论的常见形态。',
  }),
  mark({
    comment_id: 'demo-no-cover',
    title: '无封面',
    my_rating: 5,
    intro: '封面缺失时显示虚线占位框，占位尺寸与真实封面一致。',
    comment: '封面为空串时的兜底形态。',
  }),
  mark({
    comment_id: 'demo-minimal',
    title: '极简（仅标题与日期）',
  }),
]

const LONG_TEXT_MARKS = [
  mark({
    comment_id: 'demo-long-title',
    title: LONG_TITLE,
    my_rating: 4,
    cover: COVER_A,
    intro: '标题超长时自动换行，星级始终保持在标题行右侧。',
  }),
  mark({
    comment_id: 'demo-long-alt',
    title: '超长又名',
    alt_title: `${LONG_TITLE} English Extended Edition`,
    my_rating: 3,
    cover: COVER_B,
  }),
  mark({
    comment_id: 'demo-long-intro',
    title: '超长简介',
    intro: LONG_INTRO,
    my_rating: 5,
    cover: COVER_C,
  }),
  mark({
    comment_id: 'demo-long-comment',
    title: '超长短评',
    comment: LONG_COMMENT,
    my_rating: 5,
    cover: COVER_A,
  }),
]

const NARROW_MARKS = [
  mark({
    comment_id: 'demo-narrow-full',
    title: '窄容器 · 完整字段',
    alt_title: 'Narrow Viewport',
    my_rating: 5,
    cover: COVER_B,
    intro: LONG_INTRO,
    comment: LONG_COMMENT,
  }),
  mark({
    comment_id: 'demo-narrow-minimal',
    title: '窄容器 · 极简',
    date: '2026-10-01',
  }),
]

function CardList({ items }: { items: DoubanMark[] }) {
  return (
    <ul className="mark-card-demo__list">
      {items.map((item) => (
        <MarkCard key={item.comment_id} mark={item} />
      ))}
    </ul>
  )
}

function DemoSection({
  title,
  description,
  children,
}: {
  title: string
  description?: string
  children: React.ReactNode
}) {
  return (
    <section className="mark-card-demo__section">
      <h2 className="mark-card-demo__section-title">{title}</h2>
      {description ? (
        <p className="mark-card-demo__section-desc">{description}</p>
      ) : null}
      {children}
    </section>
  )
}

/** MarkCard 组件演示页：静态 mock 数据覆盖各字段组合与极端情况，
 *  供调整卡片样式时对照；配合顶栏主题切换可核对明暗两套配色 */
export function MarkCardDemoPage() {
  return (
    <section className="mark-card-demo">
      <h1 className="mark-card-demo__title">MarkCard 组件演示</h1>
      <p className="mark-card-demo__desc">
        纯静态 mock，不请求后端；真实数据列表见
        <Link to="/movie">看过的电影</Link>。卡片组件在
        <code>site/mark-card.tsx</code>。
      </p>

      <DemoSection
        description="所有字段齐备的基准形态；附阿文（RTL）可选标题与评分混排的 bidi 隔离回归用例"
        title="完整字段"
      >
        <CardList items={[FULL_MARK, RTL_MARK]} />
      </DemoSection>

      <DemoSection
        description="金色覆盖层按 1/5 比例裁切，验证各档填充宽度"
        title="评分 1–5 星"
      >
        <CardList items={RATING_MARKS} />
      </DemoSection>

      <DemoSection
        description="可选字段逐一缺失，直到只剩标题与日期"
        title="字段缺省"
      >
        <CardList items={MISSING_FIELD_MARKS} />
      </DemoSection>

      <DemoSection
        description="标题/又名自动换行，简介与短评不限行数，卡片随正文撑开高度"
        title="长文本完整展示"
      >
        <CardList items={LONG_TEXT_MARKS} />
      </DemoSection>

      <DemoSection
        description="约 360px 宽容器，模拟窄屏下的换行与卡片撑开表现"
        title="窄容器"
      >
        <div className="mark-card-demo__narrow">
          <CardList items={NARROW_MARKS} />
        </div>
      </DemoSection>
    </section>
  )
}
