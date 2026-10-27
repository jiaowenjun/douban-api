import { useEffect, useState } from 'react';
import { Alert, App as AntApp, Button, Card, Descriptions, Drawer, Progress, Space, Table, Tag } from 'antd';
import { DownloadOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';
import type { ProgressEntry, ResultView, TaskView } from '../types';
import { api } from '../api';
import { DimStateTag, TaskStatusTag, dimLabel, formatBytes } from '../labels';

const TERMINAL = ['completed', 'partial', 'failed'];

function DimProgress({ dim }: { dim: ProgressEntry }) {
  const done = dim.state === 'done';
  const failed = dim.state === 'error';
  // page = 已完成的页数（next_start/15）；running 时正在抓第 page+1 页
  const pageInfo = dim.pages_total
    ? ` · 第 ${dim.state === 'running' ? dim.page + 1 : dim.page}/${dim.pages_total} 页`
    : dim.page > 0
      ? ` · 第 ${dim.page} 页`
      : '';
  return (
    <div style={{ marginBottom: 12 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
        <Space size={8}>
          <span>{dimLabel(dim.category, dim.status)}</span>
          <DimStateTag state={dim.state} />
        </Space>
        <span style={{ color: '#888' }}>
          {dim.fetched}
          {dim.total != null ? ` / ${dim.total}` : ''} 条{pageInfo}
        </span>
      </div>
      <Progress
        percent={dim.percent ?? 0}
        size="small"
        status={failed ? 'exception' : done ? 'success' : 'active'}
        format={() => (dim.percent == null ? '未知' : `${dim.percent}%`)}
      />
    </div>
  );
}

export default function TaskDetailDrawer({
  taskId,
  onClose,
}: {
  taskId: string | null;
  onClose: () => void;
}) {
  const { message } = AntApp.useApp();
  const [task, setTask] = useState<TaskView | null>(null);
  const [result, setResult] = useState<ResultView | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  // 打开期间：先拉一次快照；未终态则订阅 SSE 推流（进度变更即达），
  // 连接失败（代理不支持流式/网络中断）自动降级为 2s 轮询；终态后拉取结果元数据
  useEffect(() => {
    if (!taskId) {
      setTask(null);
      setResult(null);
      setLoadError(null);
      return;
    }
    let stopped = false;
    let timer: number | undefined;
    let es: EventSource | undefined;

    const fetchResult = async () => {
      try {
        setResult(await api.getResult(taskId));
      } catch (e) {
        if (!stopped) setLoadError(`结果加载失败：${(e as Error).message}`);
      }
    };

    /** 应用任务视图；返回是否终态 */
    const applyTask = (t: TaskView): boolean => {
      if (stopped) return true;
      setTask(t);
      setLoadError(null);
      if (TERMINAL.includes(t.status)) {
        void fetchResult();
        return true;
      }
      return false;
    };

    const startPolling = () => {
      const tick = async () => {
        let terminal = false;
        try {
          terminal = applyTask(await api.getTask(taskId));
        } catch (e) {
          if (!stopped) setLoadError((e as Error).message);
        }
        if (!terminal && !stopped) timer = window.setTimeout(tick, 2000);
      };
      void tick();
    };

    (async () => {
      try {
        if (applyTask(await api.getTask(taskId))) return; // 打开时已是终态
      } catch (e) {
        if (stopped) return;
        setLoadError((e as Error).message);
        timer = window.setTimeout(startPolling, 3000);
        return;
      }

      es = new EventSource(api.taskEventsUrl(taskId));
      const onDelta = (ev: Event) => {
        try {
          applyTask(JSON.parse((ev as MessageEvent).data) as TaskView);
        } catch {
          /* 忽略无法解析的数据帧 */
        }
      };
      es.addEventListener('progress', onDelta);
      es.addEventListener('done', (ev) => {
        onDelta(ev);
        es?.close();
      });
      es.addEventListener('error', (ev) => {
        // 两类 error 事件：业务终态失败（有 data）与连接错误（无 data，EventSource 会自动重连）
        if ((ev as MessageEvent).data != null) {
          onDelta(ev);
          es?.close();
          return;
        }
        // 连接不可用：关闭并降级轮询，避免 EventSource 无限重连
        es?.close();
        es = undefined;
        if (!stopped) startPolling();
      });
    })();

    return () => {
      stopped = true;
      es?.close();
      if (timer) clearTimeout(timer);
    };
  }, [taskId]);

  const download = async (name: string, url: string) => {
    try {
      await api.downloadFile(url, name);
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  return (
    <Drawer open={!!taskId} onClose={onClose} width={560} title={task ? `任务 ${task.task_id}` : '任务详情'}>
      {loadError && <Alert type="error" showIcon style={{ marginBottom: 16 }} message={loadError} />}
      {!task && !loadError && <span>加载中…</span>}
      {task && (
        <>
          <Descriptions size="small" column={2} bordered style={{ marginBottom: 16 }}>
            <Descriptions.Item label="用户">{task.user}</Descriptions.Item>
            <Descriptions.Item label="状态">
              <TaskStatusTag status={task.status} />
            </Descriptions.Item>
            <Descriptions.Item label="已抓条目">{task.items_count}</Descriptions.Item>
            <Descriptions.Item label="总进度">
              {task.percent == null ? '—' : `${task.percent}%`}
            </Descriptions.Item>
            <Descriptions.Item label="创建时间" span={2}>
              {dayjs(task.created_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
            {task.finished_at && (
              <Descriptions.Item label="完成时间">
                {dayjs(task.finished_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
            )}
            {task.expires_at && (
              <Descriptions.Item label="文件过期">
                {dayjs(task.expires_at).format('YYYY-MM-DD HH:mm')}
              </Descriptions.Item>
            )}
          </Descriptions>

          {task.error && (
            <Alert
              type={task.status === 'partial' ? 'warning' : 'error'}
              showIcon
              style={{ marginBottom: 16 }}
              message={`${task.error.code}：${task.error.message}`}
              description={
                task.error.failed_dimensions?.length
                  ? `失败维度：${task.error.failed_dimensions.map((d) => dimLabel(d.category, d.status)).join('、')}`
                  : undefined
              }
            />
          )}

          <Card size="small" title="维度进度" style={{ marginBottom: 16 }}>
            {task.progress.map((dim) => (
              <DimProgress key={`${dim.category}-${dim.status}`} dim={dim} />
            ))}
          </Card>

          {result && (
            <Card size="small" title="导出结果">
              <Descriptions size="small" column={1} style={{ marginBottom: 8 }}>
                <Descriptions.Item label="汇总">
                  共 {result.summary.total} 条，含短评 {result.summary.with_comment} 条
                </Descriptions.Item>
                {Object.entries(result.summary.by_category_status).map(([k, v]) => (
                  <Descriptions.Item key={k} label={k}>
                    {v} 条
                  </Descriptions.Item>
                ))}
              </Descriptions>
              <Table
                size="small"
                rowKey="name"
                pagination={false}
                dataSource={result.files}
                columns={[
                  {
                    title: '文件',
                    dataIndex: 'name',
                    render: (_, f) => (
                      <Space size={4}>
                        <Tag>{f.format.toUpperCase()}</Tag>
                        {f.name}
                      </Space>
                    ),
                  },
                  { title: '条目', dataIndex: 'items', width: 70 },
                  { title: '大小', dataIndex: 'size', width: 90, render: (v) => formatBytes(v) },
                  {
                    title: '',
                    width: 80,
                    render: (_, f) => (
                      <Button
                        type="link"
                        size="small"
                        icon={<DownloadOutlined />}
                        onClick={() => download(f.name, f.url)}
                      >
                        下载
                      </Button>
                    ),
                  },
                ]}
              />
            </Card>
          )}
        </>
      )}
    </Drawer>
  );
}
