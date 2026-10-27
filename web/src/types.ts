// 与 src/server/service.rs 的响应结构一一对应

export type TaskStatus = 'queued' | 'running' | 'completed' | 'partial' | 'failed';

/** GET /syncs 列表项（摘要字段） */
export interface TaskSummary {
  url: string;
  task_id: string;
  user: string;
  status: TaskStatus;
  items_count: number;
  created_at: string;
  /** manual = 手动创建；auto = 订阅全量同步自动创建（默认列表不展示） */
  source: string;
}

export interface TaskListResponse {
  tasks: TaskSummary[];
  next_cursor: number | null;
  has_more: boolean;
}

/** GET /syncs/{id} 与 POST /syncs 响应 */
export interface ProgressEntry {
  category: string;
  status: string;
  state: 'queued' | 'running' | 'done' | 'error' | string;
  fetched: number;
  total: number | null;
  /** 0-100；done 恒 100，未知总数/出错为 null，进行中封顶 99 */
  percent: number | null;
  next_start: number;
  page: number;
  pages_total: number | null;
}

export interface TaskError {
  code: string;
  message: string;
  failed_dimensions?: { category: string; status: string }[];
}

export interface TaskView {
  task_id: string;
  user: string;
  status: TaskStatus;
  request: { categories: string[]; statuses: string[]; refresh: boolean };
  progress: ProgressEntry[];
  items_count: number;
  error: TaskError | null;
  percent: number | null;
  created_at: string;
  started_at: string | null;
  finished_at: string | null;
  expires_at: string | null;
  result_url: string;
}

export interface CreateTaskResponse extends TaskView {
  cached_dimensions?: number;
  created_dimensions?: number;
}

/** 任务结果中一个完成维度的汇总 */
export interface ResultDim {
  category: string;
  status: string;
  items: number;
  with_comment: number;
}

export interface ResultView {
  task_id: string;
  user: string;
  summary: {
    total: number;
    with_comment: number;
    by_category_status: Record<string, number>;
  };
  dimensions: ResultDim[];
  expires_at: string | null;
}

/** Record 的 12 字段（与 CLI 导出一致） */
export interface RecordItem {
  category: string;
  status: string;
  title: string;
  alt_title: string;
  my_rating: number | null;
  date: string;
  comment: string;
  subject_url: string;
  subject_id: string;
  cover: string;
  comment_id: string;
  intro: string;
}

export interface MarksResponse {
  user: string;
  category: string;
  status: string;
  cached_at: string;
  fresh: boolean;
  total: number;
  page: number;
  page_size: number;
  items: RecordItem[];
}

export interface ServerInfo {
  name: string;
  version: string;
  crawl: {
    min_interval_seconds: number;
    circuit: {
      threshold: number;
      cooldown_seconds: number;
      open: boolean;
      remaining_ms: number;
    };
  };
  cache: {
    ttl_seconds: number;
    retention_task_days: number;
  };
  queue: { queued: number; running: number };
  subscriptions: { count: number; enabled: number };
  auth_enabled: boolean;
  cookie_enabled: boolean;
}

export interface CreateTaskBody {
  user: string;
  categories: string[];
  statuses: string[];
  refresh: boolean;
}

/** 订阅同步（RSS 短周期增量 + 长周期全量 auto 任务） */
export interface Subscription {
  uid: string;
  enabled: boolean;
  rss_interval_secs: number;
  full_interval_secs: number;
  next_rss_at: string | null;
  next_full_at: string | null;
  last_rss_at: string | null;
  last_full_at: string | null;
  last_error: string | null;
  dimensions: number;
  records: number;
  created_at: string;
}

export interface SubscriptionListResponse {
  subscriptions: Subscription[];
}
