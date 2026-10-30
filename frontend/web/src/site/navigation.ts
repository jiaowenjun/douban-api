/** 站点书影音类别键（与路由段一致） */
export type MarksCategory = 'movie' | 'book' | 'music'

/** 「全部标记」伪类别：路由段/筛选树顶导航节点的 tag，书影音三类别
 *  合并视图；非真实豆瓣类别，不入 MARKS_CATEGORIES——筛选树类别根与
 *  统计区仍按三类 */
export const ALL_MARKS_TAG = 'all'

/** 树/路由可导航目标：真实类别或「全部标记」 */
export type NavTarget = MarksCategory | typeof ALL_MARKS_TAG

/** 站点只展示「看过」（collect）的标记 */
export const MARKS_STATUS = 'collect'

export const DEFAULT_MARKS_PATH = '/movie'

/** 侧边栏条目顺序 */
export const MARKS_CATEGORIES: readonly MarksCategory[] = [
  'movie',
  'book',
  'music',
]

/** 类别中文名；movie 值「影视」仅作 isMarksCategory 的键与不可达
 *  兜底——电影/电视剧是真一级（裸 /movie 由 App 的 MarksRoute 重定向
 *  补形态步，展示名见 moviePageTitle），聚合名不再对外展示；book/
 *  music 为类别根/标题/统计行用名 */
export const CATEGORY_NOUNS: Record<MarksCategory, string> = {
  book: '图书',
  movie: '影视',
  music: '音乐',
}

/** movie 缺形态步（is_tv）时的规范默认形态——裸 /movie 由 App 的
 *  MarksRoute 重定向补此步，标题/文案兜底同用（值硬编码中文与
 *  marks-page/facet-tree 的形态值比对同款） */
export const DEFAULT_MOVIE_KIND = '电影'

/** movie 页展示标题（document.title/占位文案）：形态即一级（电影/
 *  电视剧），无「影视」聚合名；kind 为 is_tv 步值，缺省兜底 */
export function moviePageTitle(kind: string | undefined): string {
  return `看过的${kind ?? DEFAULT_MOVIE_KIND}`
}

/** 页面标题动词：看过的影视 / 读过的图书 / 听过的音乐 */
const CATEGORY_VERBS: Record<MarksCategory, string> = {
  book: '读',
  movie: '看',
  music: '听',
}

/** 列表 meta 行完成态动词：芝麻 已看过 183 部影视 */
export const CATEGORY_PAST_VERBS: Record<MarksCategory, string> = {
  book: '已读过',
  movie: '已看过',
  music: '已听过',
}

/** 列表 meta 行量词名词，与 CATEGORY_PAST_VERBS 配对拼「已读过 55
 *  本书」；电影/电视剧形态提升为名词（…部电影/…部电视剧，见
 *  marks-page meta 行），movie 值仅未分类形态兜底（富集完成后归零） */
export const CATEGORY_UNIT_NOUNS: Record<MarksCategory, string> = {
  book: '本书',
  movie: '部影视',
  music: '张专辑',
}

export function isMarksCategory(value: string): value is MarksCategory {
  return value in CATEGORY_NOUNS
}

export type MarksView = {
  activeItemId: string
  category: NavTarget
  pageTitle: string
}

/** 校验路由参数并派生导航高亮项与页面标题；非法值返回 null。
 *  all 为三类别合并视图（全部标记），无分面维度 */
export function resolveMarksView(
  category: string | undefined,
): MarksView | null {
  if (category === ALL_MARKS_TAG) {
    return {
      activeItemId: ALL_MARKS_TAG,
      category: ALL_MARKS_TAG,
      pageTitle: '全部标记',
    }
  }
  if (!category || !isMarksCategory(category)) return null

  return {
    activeItemId: category,
    category,
    // movie 标题实际由 App 按形态覆盖（形态步经 MarksRoute 保证存在），
    // 此处仅无 facet 一帧的兜底；book/music 为「动词过的类别名」
    pageTitle:
      category === 'movie'
        ? moviePageTitle(undefined)
        : `${CATEGORY_VERBS[category]}过的${CATEGORY_NOUNS[category]}`,
  }
}
