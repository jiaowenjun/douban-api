import { Tag } from 'antd';

export const CATEGORY_OPTIONS = [
  { value: 'movie', label: '电影' },
  { value: 'book', label: '书本' },
  { value: 'music', label: '音乐' },
];

export const STATUS_OPTIONS = [
  { value: 'collect', label: '看过 / 读过 / 听过' },
  { value: 'do', label: '在看 / 在读 / 在听' },
  { value: 'wish', label: '想看 / 想读 / 想听' },
];

export const TASK_STATUS_META: Record<string, { label: string; color: string }> = {
  queued: { label: '排队中', color: 'default' },
  running: { label: '抓取中', color: 'processing' },
  completed: { label: '已完成', color: 'success' },
  partial: { label: '部分成功', color: 'warning' },
  failed: { label: '失败', color: 'error' },
};

export const TASK_STATUSES = Object.keys(TASK_STATUS_META);

export function TaskStatusTag({ status }: { status: string }) {
  const meta = TASK_STATUS_META[status];
  return <Tag color={meta?.color}>{meta?.label ?? status}</Tag>;
}

/** 维度标签：电影·看过 / 书本·读过 / 音乐·听过 */
export function dimLabel(category: string, status: string) {
  const cat = CATEGORY_OPTIONS.find((c) => c.value === category)?.label ?? category;
  const verb = { movie: '看', book: '读', music: '听' }[category] ?? '标记';
  const st = { collect: `${verb}过`, do: `在${verb}`, wish: `想${verb}` }[status] ?? status;
  return `${cat}·${st}`;
}

export function DimStateTag({ state }: { state: string }) {
  const meta: Record<string, { label: string; color: string }> = {
    queued: { label: '排队', color: 'default' },
    running: { label: '抓取中', color: 'processing' },
    done: { label: '完成', color: 'success' },
    error: { label: '失败', color: 'error' },
  };
  const m = meta[state];
  return <Tag color={m?.color}>{m?.label ?? state}</Tag>;
}
