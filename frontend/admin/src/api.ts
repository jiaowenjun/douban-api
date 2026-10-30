import type {
  CreateTaskBody,
  CreateTaskResponse,
  MarksResponse,
  ResultView,
  ServerInfo,
  Subject,
  SubjectJobListResponse,
  SubjectListResponse,
  Subscription,
  SubscriptionListResponse,
  TaskListResponse,
  TaskView,
  UserProfile,
} from './types';

// 连接配置存 localStorage：默认同源 /api/v1（开发期由 Vite 代理），
// 也可填完整地址（如 http://192.168.1.10:8080/api/v1，需服务器开 CORS）
const BASE_KEY = 'douban-admin:api-base';
const TOKEN_KEY = 'douban-admin:token';
const USER_KEY = 'douban-admin:user';

/* 默认基址经 VITE_API_BASE_URL 构建期注入（生产 /douban-admin/api/v1，dev 回退
   /api/v1 走 Vite proxy）；localStorage 覆盖机制保留（连接弹框仍可改） */
export const getApiBase = () =>
  localStorage.getItem(BASE_KEY) || import.meta.env.VITE_API_BASE_URL || '/api/v1';
export const setApiBase = (v: string) => localStorage.setItem(BASE_KEY, v);
export const getToken = () => localStorage.getItem(TOKEN_KEY) || '';
export const setToken = (v: string) => localStorage.setItem(TOKEN_KEY, v);

/** 解析服务端返回的相对 API 路径（如 /api/v1/covers?...）为可请求的地址。
 *  API base 为跨源完整地址时（dist 静态部署 + CORS），相对路径会错误地落在前端
 *  自己的 origin 上，需按 base 的 origin 解析；开启鉴权时 img 标签带不了
 *  Authorization 头，追加 ?token= 查询参数兜底（服务端已支持）。 */
export function resolveApiUrl(path: string): string {
  if (!path || !path.startsWith('/')) return path;
  const base = getApiBase();
  let url: URL;
  try {
    url = new URL(path, /^https?:\/\//.test(base) ? base : window.location.origin);
  } catch {
    return path;
  }
  const token = getToken();
  if (token && !url.searchParams.has('token')) {
    url.searchParams.set('token', token);
  }
  return url.href;
}

/** 单用户站点的用户身份：在「连接」弹框设置一次，全站复用 */
export const getUser = () => localStorage.getItem(USER_KEY) || '';
export const setUser = (v: string) => localStorage.setItem(USER_KEY, v);

/** 与服务端 backend/src/crawler.rs::parse_uid 一致：接受纯 ID 或个人主页链接，归一化为纯 uid */
export function parseUserInput(raw: string): string | null {
  const input = raw.trim();
  const isValidId = (s: string) => /^[A-Za-z0-9._-]+$/.test(s);
  const i = input.indexOf('/people/');
  if (i >= 0) {
    const id = input.slice(i + 8).split(/[/?#]/)[0];
    if (isValidId(id)) return id;
  }
  return isValidId(input) ? input : null;
}

export class ApiError extends Error {
  code: string;
  status: number;
  constructor(message: string, code: string, status: number) {
    super(message);
    this.code = code;
    this.status = status;
  }
}

async function parseError(res: Response): Promise<ApiError> {
  let code = 'HTTP_ERROR';
  let message = `请求失败（HTTP ${res.status}）`;
  try {
    const body = await res.json();
    if (body?.error?.code) code = body.error.code;
    if (body?.error?.message) message = body.error.message;
  } catch {
    /* 响应体不是 JSON（如网关错误页）时保留默认信息 */
  }
  return new ApiError(message, code, res.status);
}

function authHeaders(): Record<string, string> {
  const token = getToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

export async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const res = await fetch(getApiBase() + path, {
    ...init,
    headers: {
      ...authHeaders(),
      ...(init.body ? { 'Content-Type': 'application/json' } : {}),
    },
  });
  if (!res.ok) throw await parseError(res);
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** /healthz 不在 /api/v1 前缀下，不能走 request()（它会再拼一次 base） */
export const health = async () => {
  const origin = getApiBase().replace(/\/api\/v1\/?$/, '').replace(/\/$/, '');
  const res = await fetch(origin + '/healthz', { headers: authHeaders() });
  if (!res.ok) throw await parseError(res);
  return (await res.json()) as { status: string };
};

export const api = {
  serverInfo: () => request<ServerInfo>('/server/info'),

  listTasks: (query: { user?: string; status?: string; limit?: number; cursor?: number }) => {
    const qs = new URLSearchParams();
    if (query.user) qs.set('user', query.user);
    if (query.status) qs.set('status', query.status);
    qs.set('limit', String(query.limit ?? 100));
    if (query.cursor != null) qs.set('cursor', String(query.cursor));
    return request<TaskListResponse>(`/syncs?${qs.toString()}`);
  },

  getTask: (taskId: string) => request<TaskView>(`/syncs/${taskId}`),

  /** SSE 订阅地址（EventSource 无法设置 Header，token 经 ?token= 查询参数传递） */
  taskEventsUrl: (taskId: string) => {
    const token = getToken();
    const q = token ? `?token=${encodeURIComponent(token)}` : '';
    return `${getApiBase()}/syncs/${encodeURIComponent(taskId)}/events${q}`;
  },

  createTask: (body: CreateTaskBody) =>
    request<CreateTaskResponse>('/syncs', { method: 'POST', body: JSON.stringify(body) }),

  deleteTask: (taskId: string) => request<void>(`/syncs/${taskId}`, { method: 'DELETE' }),

  getResult: (taskId: string) => request<ResultView>(`/syncs/${taskId}/result`),

  getMarks: (
    uid: string,
    category: string,
    status: string,
    page: number,
    pageSize: number,
    opts?: { expandSubject?: boolean },
  ) =>
    request<MarksResponse>(
      `/users/${encodeURIComponent(uid)}/marks?category=${category}&status=${status}` +
        `&page=${page}&page_size=${pageSize}` +
        (opts?.expandSubject ? '&expand=subject' : ''),
    ),

  /** 条目元信息（rexxar 富集缓存）：列表（分页 + 类别/标题过滤 + 队列进度计数） */
  listSubjects: (query: { category?: string; q?: string; page?: number; pageSize?: number }) => {
    const qs = new URLSearchParams();
    if (query.category && query.category !== 'all') qs.set('category', query.category);
    if (query.q) qs.set('q', query.q);
    qs.set('page', String(query.page ?? 1));
    qs.set('page_size', String(query.pageSize ?? 20));
    return request<SubjectListResponse>(`/subjects?${qs.toString()}`);
  },

  getSubject: (category: string, subjectId: string) =>
    request<Subject>(`/subjects/${encodeURIComponent(category)}/${encodeURIComponent(subjectId)}`),

  /** 无视 TTL 强制重抓单条目（202 异步，worker 空闲时段执行） */
  refreshSubject: (category: string, subjectId: string) =>
    request<{ queued: boolean }>(
      `/subjects/${encodeURIComponent(category)}/${encodeURIComponent(subjectId)}/refresh`,
      { method: 'POST' },
    ),

  /** 存量回填：对 records 全部 (uid, category) 跑富集集合差（幂等；只入缺失条目） */
  backfillSubjects: () =>
    request<{ queued_jobs: number; uid_category_pairs: number }>('/subjects/backfill', {
      method: 'POST',
    }),

  /** 富集任务列表（state 逗号分隔多值过滤；附各状态计数） */
  listSubjectJobs: (query: {
    state?: string;
    category?: string;
    q?: string;
    page?: number;
    pageSize?: number;
  }) => {
    const qs = new URLSearchParams();
    if (query.state && query.state !== 'all') qs.set('state', query.state);
    if (query.category && query.category !== 'all') qs.set('category', query.category);
    if (query.q) qs.set('q', query.q);
    qs.set('page', String(query.page ?? 1));
    qs.set('page_size', String(query.pageSize ?? 20));
    return request<SubjectJobListResponse>(`/subjects/jobs?${qs.toString()}`);
  },

  /** 删除单个富集任务（queued 即取消该条目；终态删除后可在下次同步重新入队） */
  deleteSubjectJob: (category: string, subjectId: string) =>
    request<void>(
      `/subjects/jobs/${encodeURIComponent(category)}/${encodeURIComponent(subjectId)}`,
      { method: 'DELETE' },
    ),

  /** 按状态批量删除任务；中断队列 = states ['queued','running'] */
  clearSubjectJobs: (body: { states: string[]; category?: string }) =>
    request<{ deleted: number }>('/subjects/jobs/clear', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  /** 批量重排：failed（重试失败终态）/ not_found（重试已删除条目）/ stale（强制重抓过期条目） */
  requeueSubjectJobs: (scope: 'failed' | 'not_found' | 'stale', category?: string) =>
    request<{ queued: number }>('/subjects/jobs/requeue', {
      method: 'POST',
      body: JSON.stringify({ scope, category }),
    }),

  getUserProfile: (uid: string) =>
    request<UserProfile>(`/users/${encodeURIComponent(uid)}/profile`),

  listSubscriptions: () =>
    request<SubscriptionListResponse>('/subscriptions'),

  createSubscription: (uid: string, body: { rss_interval_secs?: number; full_interval_secs?: number }) =>
    request<Subscription>(`/subscriptions/${encodeURIComponent(uid)}`, {
      method: 'PUT',
      body: JSON.stringify(body),
    }),

  deleteSubscription: (uid: string) =>
    request<void>(`/subscriptions/${encodeURIComponent(uid)}`, { method: 'DELETE' }),
};
