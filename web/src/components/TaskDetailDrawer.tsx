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
  const pageInfo = dim.pages_total
    ? ` · 第 ${Math.max(dim.page, 1)}/${dim.pages_total} 页`
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

  // 打开期间轮询：未终态每 2s 刷新，终态后停并拉取结果元数据
  useEffect(() => {
    if (!taskId) {
      setTask(null);
      setResult(null);
      setLoadError(null);
      return;
    }
    let stopped = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const t = await api.getTask(taskId);
        if (stopped) return;
        setTask(t);
        setLoadError(null);
        if (TERMINAL.includes(t.status)) {
          try {
            setResult(await api.getResult(taskId));
          } catch (e) {
            if (!stopped) setLoadError(`结果加载失败：${(e as Error).message}`);
          }
          return;
        }
        timer = window.setTimeout(tick, 2000);
      } catch (e) {
        if (stopped) return;
        setLoadError((e as Error).message);
        timer = window.setTimeout(tick, 5000);
      }
    };

    setTask(null);
    setResult(null);
    setLoadError(null);
    tick();
    return () => {
      stopped = true;
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
