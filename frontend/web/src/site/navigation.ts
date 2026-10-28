import type {
  SiteNavigationGroup,
  SiteNavigationItem,
} from 'weimo-ui-template/components/site-template'

export type MarksCategory = 'movie' | 'book' | 'music'
export type MarksStatus = 'collect' | 'do' | 'wish'

export const DEFAULT_MARKS_PATH = '/movie/collect'

export const CATEGORY_NOUNS: Record<MarksCategory, string> = {
  book: '图书',
  movie: '电影',
  music: '音乐',
}

const CATEGORY_VERBS: Record<MarksCategory, string> = {
  book: '读',
  movie: '看',
  music: '听',
}

/** 组内导航短标签：看过 / 在看 / 想看（分组标题已含领域名词） */
const STATUS_LABELS: Record<MarksStatus, string> = {
  collect: '过',
  do: '在',
  wish: '想',
}

function statusLabel(category: MarksCategory, status: MarksStatus): string {
  const verb = CATEGORY_VERBS[category]
  const prefix = STATUS_LABELS[status]

  return status === 'collect' ? verb + prefix : prefix + verb
}

function isMarksCategory(value: string): value is MarksCategory {
  return value in CATEGORY_NOUNS
}

function isMarksStatus(value: string): value is MarksStatus {
  return value in STATUS_LABELS
}

export type MarksView = {
  activeItemId: string
  category: MarksCategory
  pageTitle: string
  status: MarksStatus
}

/** 校验路由参数并派生导航高亮项与页面标题；非法组合返回 null */
export function resolveMarksView(
  category: string | undefined,
  status: string | undefined,
): MarksView | null {
  if (!category || !status) return null
  if (!isMarksCategory(category) || !isMarksStatus(status)) return null

  const label = statusLabel(category, status)

  return {
    activeItemId: `${category}-${status}`,
    category,
    pageTitle: `${label}的${CATEGORY_NOUNS[category]}`,
    status,
  }
}

function marksItems(
  category: MarksCategory,
  statuses: readonly MarksStatus[],
): SiteNavigationItem[] {
  return statuses.map((status) => ({
    href: `#/${category}/${status}`,
    id: `${category}-${status}`,
    label: statusLabel(category, status),
  }))
}

export const MARKS_NAVIGATION_GROUPS: readonly SiteNavigationGroup[] = (
  [
    ['movie', ['collect', 'do', 'wish']],
    ['book', ['collect', 'do', 'wish']],
    ['music', ['collect', 'do', 'wish']],
  ] as const
).map(([category, statuses]) => ({
  id: category,
  items: marksItems(category, statuses),
  title: CATEGORY_NOUNS[category],
}))
