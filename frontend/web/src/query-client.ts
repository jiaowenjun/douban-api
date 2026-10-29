import { QueryClient } from '@tanstack/react-query'

/** 缓存新鲜期：数据是后端 24h TTL 的只读缓存，web 端无 mutation，
 *  10 分钟内切类别/筛选/翻页一律命中缓存不发请求（对齐 weimo-biji
 *  的 WORKSPACE_CACHE_STALE_TIME_MS） */
export const DOUBAN_CACHE_STALE_TIME_MS = 10 * 60_000

/** 缓存保留期：卸载的 observer 再挂回（如路由切回）时仍可即时复用 */
export const DOUBAN_CACHE_GC_TIME_MS = 30 * 60_000

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
})
