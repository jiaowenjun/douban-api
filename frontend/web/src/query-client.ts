import { QueryClient } from '@tanstack/react-query'

/** Published content stays fresh until its version changes; retention remains bounded. */
export const DOUBAN_CACHE_STALE_TIME_MS = Infinity
export const DOUBAN_CACHE_GC_TIME_MS = 7 * 24 * 60 * 60_000
export const DOUBAN_CACHE_MAX_AGE_MS = 30 * 24 * 60 * 60_000

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: false,
      gcTime: DOUBAN_CACHE_GC_TIME_MS,
    },
  },
})
