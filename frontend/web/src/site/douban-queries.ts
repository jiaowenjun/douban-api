/** 服务端状态查询层：TanStack Query 封装 api.ts 的四个只读端点，
 *  技术栈对齐 weimo-biji（react-query v5 纯内存缓存，无 persister /
 *  devtools）。关键决策——queryFn 返回 api 层信封（MarksResult、
 *  DoubanProfile | null 等）而不抛错：not-cached 与 error 的区分、
 *  「失败即 null 静默降级」语义原样进缓存；retry 永不触发，429 不会
 *  被自动重试放大后端的每 IP 限流。key 全部带 uid 作用域，登出/切号
 *  由 provider 调 removeQueries(doubanQueryKeys.all) 整体清空 */
import { useQuery } from '@tanstack/react-query'

import {
  DOUBAN_CACHE_GC_TIME_MS,
  DOUBAN_CACHE_STALE_TIME_MS,
  queryClient,
} from '../query-client'
import {
  fetchDoubanMarks,
  fetchDoubanProfile,
  fetchDoubanStats,
  fetchDoubanSubjectDims,
} from './api'
import type { MarksResult } from './api'
import type { MarksCategory } from './navigation'

/** 列表分页大小：查询 key 与调用方共用同一常量 */
export const MARKS_PAGE_SIZE = 20

/** 层级 query key 工厂（对齐 biji workspaceQueryKeys 模式）：uid 作用域
 *  → 数据域 → 参数。marks 的 ids 为筛选命中 id 切片 join（subject_id
 *  纯数字不含逗号），null = 无筛选 */
export const doubanQueryKeys = {
  all: ['douban'] as const,
  uid: (uid: string) => ['douban', uid] as const,
  profile: (uid: string) => [...doubanQueryKeys.uid(uid), 'profile'] as const,
  stats: (uid: string) => [...doubanQueryKeys.uid(uid), 'stats'] as const,
  dims: (uid: string, category: string) =>
    [...doubanQueryKeys.uid(uid), 'dims', category] as const,
  marksPage: (
    uid: string,
    category: string,
    page: number,
    ids: readonly string[] | null | undefined,
  ) =>
    [
      ...doubanQueryKeys.uid(uid),
      'marks',
      category,
      { page, pageSize: MARKS_PAGE_SIZE, ids: ids?.join(',') ?? null },
    ] as const,
}

/** 逐 query 显式缓存参数（对齐 biji：全局 defaultOptions 只关窗口聚焦
 *  重拉与 retry 次数，新鲜期/保留期在查询层声明） */
const cacheOptions = {
  staleTime: DOUBAN_CACHE_STALE_TIME_MS,
  gcTime: DOUBAN_CACHE_GC_TIME_MS,
} as const

/** 用户资料（头像/用户名）；失败缓存为 null——界面回退 uid 展示 */
export function useDoubanProfileQuery(uid: string | null) {
  return useQuery({
    queryKey: doubanQueryKeys.profile(uid ?? ''),
    queryFn: () => fetchDoubanProfile(uid ?? ''),
    ...cacheOptions,
    enabled: uid !== null,
  })
}

/** 标记统计（按类别计数 + 按日计数）；失败缓存为 null——侧边栏统计
 *  区静默隐藏 */
export function useDoubanStatsQuery(uid: string | null) {
  return useQuery({
    queryKey: doubanQueryKeys.stats(uid ?? ''),
    queryFn: () => fetchDoubanStats(uid ?? ''),
    ...cacheOptions,
    enabled: uid !== null,
  })
}

/** 条目维度投影（筛选树数据源）；失败/未同步缓存为 null——树该类别
 *  分支隐藏。侧边栏（三类别并行）与列表页（当前类别）共用同一 key，
 *  是消除重复请求的核心 */
export function useDoubanDimsQuery(
  uid: string | null,
  category: MarksCategory | null,
) {
  return useQuery({
    queryKey: doubanQueryKeys.dims(uid ?? '', category ?? ''),
    queryFn: () => fetchDoubanSubjectDims(uid ?? '', category ?? ''),
    ...cacheOptions,
    enabled: uid !== null && category !== null,
  })
}

/** 标记页查询（列表首页用）。ids 为筛选命中的当页 id 切片（顺序权威
 *  在前端），无筛选为 undefined；是否发请求由调用方经 enabled 门控
 *  （如筛选态须等 dims 就绪才算出 ids） */
export function useDoubanMarksPageQuery(args: {
  uid: string | null
  category: string | null
  page: number
  ids?: readonly string[] | null
  enabled: boolean
}) {
  const uid = args.uid ?? ''
  const category = args.category ?? ''
  return useQuery({
    queryKey: doubanQueryKeys.marksPage(uid, category, args.page, args.ids),
    queryFn: () =>
      fetchDoubanMarks(uid, category, args.page, MARKS_PAGE_SIZE, args.ids),
    ...cacheOptions,
    enabled: args.enabled,
  })
}

/** 「加载更多」的命令式取页：fetchQuery 命中新鲜缓存直接返回（翻回
 *  已看页、筛选切回来零请求），否则请求并写入同一缓存 */
export function fetchDoubanMarksPage(args: {
  uid: string
  category: string
  page: number
  ids?: readonly string[] | null
}): Promise<MarksResult> {
  return queryClient.fetchQuery({
    queryKey: doubanQueryKeys.marksPage(
      args.uid,
      args.category,
      args.page,
      args.ids,
    ),
    queryFn: () =>
      fetchDoubanMarks(
        args.uid,
        args.category,
        args.page,
        MARKS_PAGE_SIZE,
        args.ids,
      ),
    ...cacheOptions,
  })
}
