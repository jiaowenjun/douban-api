import type {
  CreateTaskBody,
  CreateTaskResponse,
  MarksResponse,
  ResultView,
  ServerInfo,
  Subscription,
  SubscriptionListResponse,
  TaskListResponse,
  TaskView,
} from './types';

// 连接配置存 localStorage：默认同源 /api/v1（开发期由 Vite 代理），
// 也可填完整地址（如 http://192.168.1.10:8080/api/v1，需服务器开 CORS）
const BASE_KEY = 'douban-admin:api-base';
const TOKEN_KEY = 'douban-admin:token';
const USER_KEY = 'douban-admin:user';

export const getApiBase = () => localStorage.getItem(BASE_KEY) || '/api/v1';
export const setApiBase = (v: string) => localStorage.setItem(BASE_KEY, v);
export const getToken = () => localStorage.getItem(TOKEN_KEY) || '';
export const setToken = (v: string) => localStorage.setItem(TOKEN_KEY, v);

/** 单用户站点的用户身份：在「连接」弹框设置一次，全站复用 */
export const getUser = () => localStorage.getItem(USER_KEY) || '';
export const setUser = (v: string) => localStorage.setItem(USER_KEY, v);

/** 与服务端 crawler.rs::parse_uid 一致：接受纯 ID 或个人主页链接，归一化为纯 uid */
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

  getMarks: (uid: string, category: string, status: string, page: number, pageSize: number) =>
    request<MarksResponse>(
      `/users/${encodeURIComponent(uid)}/marks?category=${category}&status=${status}` +
        `&page=${page}&page_size=${pageSize}`,
    ),

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
