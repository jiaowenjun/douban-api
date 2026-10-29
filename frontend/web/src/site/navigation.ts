/** 站点书影音类别键（与路由段一致） */
export type MarksCategory = 'movie' | 'book' | 'music'

/** 站点只展示「看过」（collect）的标记 */
export const MARKS_STATUS = 'collect'

export const DEFAULT_MARKS_PATH = '/movie'

/** 侧边栏条目顺序 */
export const MARKS_CATEGORIES: readonly MarksCategory[] = [
  'movie',
  'book',
  'music',
]

export const CATEGORY_NOUNS: Record<MarksCategory, string> = {
  book: '图书',
  movie: '电影',
  music: '音乐',
}

/** 页面标题动词：看过的电影 / 读过的图书 / 听过的音乐 */
const CATEGORY_VERBS: Record<MarksCategory, string> = {
  book: '读',
  movie: '看',
  music: '听',
}

export function isMarksCategory(value: string): value is MarksCategory {
  return value in CATEGORY_NOUNS
}

export type MarksView = {
  activeItemId: string
  category: MarksCategory
  pageTitle: string
}

/** 校验路由参数并派生导航高亮项与页面标题；非法值返回 null */
export function resolveMarksView(
  category: string | undefined,
): MarksView | null {
  if (!category || !isMarksCategory(category)) return null

  return {
    activeItemId: category,
    category,
    pageTitle: `${CATEGORY_VERBS[category]}过的${CATEGORY_NOUNS[category]}`,
  }
}
