import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Alert, Badge, Button, Card, Col, Row, Statistic, Table, Tooltip } from 'antd';
import { PageContainer, ProCard } from '@ant-design/pro-components';
import { ReloadOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';
import type { ServerInfo, TaskSummary } from '../types';
import { api, health } from '../api';
import { TaskStatusTag } from '../labels';

export default function Overview() {
  const navigate = useNavigate();
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [online, setOnline] = useState<boolean | null>(null);
  const [connError, setConnError] = useState<string | null>(null);
  const [recent, setRecent] = useState<TaskSummary[]>([]);

  const load = async () => {
    try {
      await health();
      setOnline(true);
      setConnError(null);
    } catch (e) {
      setOnline(false);
      setConnError((e as Error).message);
      return;
    }
    try {
      setInfo(await api.serverInfo());
    } catch {
      /* healthz 通而 API 报错时在卡片里显示空值即可 */
    }
    try {
      const res = await api.listTasks({ limit: 5 });
      setRecent(res.tasks);
    } catch {
      setRecent([]);
    }
  };

  useEffect(() => {
    load();
    const timer = setInterval(load, 5000);
    return () => clearInterval(timer);
  }, []);

  const circuitOpen = info?.crawl.circuit.open ?? false;
  const circuitRemainMin =
    info ? Math.ceil(info.crawl.circuit.remaining_ms / 60000) : 0;

  return (
    <PageContainer
      title="概览"
      subTitle={info ? `${info.name} v${info.version}` : undefined}
      extra={[
        <Button key="refresh" icon={<ReloadOutlined />} onClick={load}>
          刷新
        </Button>,
      ]}
    >
      {connError && (
        <Alert
          type="error"
          showIcon
          style={{ marginBottom: 16 }}
          message="无法连接导出服务器"
          description={`请确认服务器已启动（默认 127.0.0.1:8080），或点击右上角「连接」修改 API 地址。错误：${connError}`}
        />
      )}
      {circuitOpen && (
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 16 }}
          message={`豆瓣抓取已熔断，预计 ${circuitRemainMin} 分钟后自动恢复。熔断期间新任务继续排队。`}
        />
      )}

      <Row gutter={[16, 16]}>
        <Col xs={24} sm={12} lg={6}>
          <Card size="small">
            <Statistic
              title="服务状态"
              valueRender={() =>
                online ? (
                  <Badge status="success" text="运行中" />
                ) : (
                  <Badge status="error" text="不可达" />
                )
              }
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card size="small">
            <Statistic title="队列（抓取中 / 排队）" value={info ? `${info.queue.running} / ${info.queue.queued}` : '—'} />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card size="small">
            <Statistic
              title="熔断器"
              valueRender={() =>
                info ? (
                  circuitOpen ? (
                    <Badge status="warning" text={`熔断中 · 剩余 ${circuitRemainMin} 分钟`} />
                  ) : (
                    <Badge status="success" text="正常" />
                  )
                ) : (
                  <>—</>
                )
              }
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card size="small">
            <Statistic
              title="鉴权"
              valueRender={() =>
                info ? (
                  info.auth_enabled ? (
                    <Badge status="processing" text="已启用 Token" />
                  ) : (
                    <Tooltip title="公网部署建议设置 EXPORT_SERVER_TOKEN">
                      <Badge status="default" text="未启用" />
                    </Tooltip>
                  )
                ) : (
                  <>—</>
                )
              }
            />
          </Card>
        </Col>
      </Row>

      <ProCard title="运行参数" size="small" style={{ marginTop: 16 }} headerBordered>
        {info ? (
          <div>抓取间隔 ≥ {info.crawl.min_interval_seconds}s · 缓存 TTL {Math.round(info.cache.ttl_seconds / 3600)} 小时
            · 任务保留 {info.cache.retention_task_days} 天 · 文件保留 {info.cache.retention_file_days} 天
            · 熔断阈值 {info.crawl.circuit.threshold} 次 / 冷却 {Math.round(info.crawl.circuit.cooldown_seconds / 60)} 分钟</div>
        ) : (
          <div>—</div>
        )}
      </ProCard>

      <ProCard
        title="最近任务"
        size="small"
        style={{ marginTop: 16 }}
        headerBordered
        extra={
          <Button type="link" size="small" onClick={() => navigate('/tasks')}>
            查看全部
          </Button>
        }
      >
        <Table<TaskSummary>
          size="small"
          rowKey="task_id"
          pagination={false}
          dataSource={recent}
          onRow={(r) => ({ onClick: () => navigate(`/tasks?task=${r.task_id}`), style: { cursor: 'pointer' } })}
          columns={[
            { title: '任务 ID', dataIndex: 'task_id', ellipsis: true },
            { title: '用户', dataIndex: 'user', width: 120 },
            { title: '状态', dataIndex: 'status', width: 100, render: (_, r) => <TaskStatusTag status={r.status} /> },
            { title: '条目', dataIndex: 'items_count', width: 80 },
            {
              title: '创建时间',
              dataIndex: 'created_at',
              width: 170,
              render: (_, r) => dayjs(r.created_at).format('YYYY-MM-DD HH:mm:ss'),
            },
          ]}
        />
      </ProCard>
    </PageContainer>
  );
}
