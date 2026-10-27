import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Alert,
  App as AntApp,
  Avatar,
  Button,
  Card,
  Form,
  Input,
  Rate,
  Select,
  Space,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import { PageContainer } from '@ant-design/pro-components';
import { SearchOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';
import type { MarksResponse, RecordItem } from '../types';
import { api, type ApiError } from '../api';
import { CATEGORY_OPTIONS, STATUS_OPTIONS, dimLabel } from '../labels';

const PAGE_SIZE = 20;

interface Query {
  uid: string;
  category: string;
  status: string;
}

export default function Marks() {
  const navigate = useNavigate();
  const { message } = AntApp.useApp();
  const [form] = Form.useForm<Query>();
  const [query, setQuery] = useState<Query | null>(null);
  const [page, setPage] = useState(1);
  const [data, setData] = useState<MarksResponse | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    if (!query) return;
    setLoading(true);
    try {
      setData(await api.getMarks(query.uid, query.category, query.status, page, PAGE_SIZE));
      setError(null);
    } catch (e) {
      setData(null);
      setError(e as ApiError);
    } finally {
      setLoading(false);
    }
  }, [query, page]);

  useEffect(() => {
    load();
  }, [load]);

  const onSearch = (v: Query) => {
    if (!v.uid?.trim()) {
      message.warning('请输入用户 ID');
      return;
    }
    setPage(1);
    setQuery({ uid: v.uid.trim(), category: v.category, status: v.status });
  };

  return (
    <PageContainer title="数据浏览" subTitle="直接读取服务器缓存的书影音标记数据，不触发抓取">
      <Card size="small" style={{ marginBottom: 16 }}>
        <Form form={form} layout="inline" onFinish={onSearch} initialValues={{ category: 'movie', status: 'collect' }}>
          <Form.Item name="uid" rules={[{ required: true, message: '请输入用户 ID' }]}>
            <Input placeholder="豆瓣用户 ID，如 wenjun30" style={{ width: 220 }} />
          </Form.Item>
          <Form.Item name="category">
            <Select options={CATEGORY_OPTIONS} style={{ width: 100 }} />
          </Form.Item>
          <Form.Item name="status">
            <Select options={STATUS_OPTIONS} style={{ width: 180 }} />
          </Form.Item>
          <Form.Item>
            <Button type="primary" htmlType="submit" icon={<SearchOutlined />}>
              查询
            </Button>
          </Form.Item>
        </Form>
      </Card>

      {error && (
        <Alert
          type={error.code === 'DIMENSION_NOT_CACHED' ? 'info' : 'error'}
          showIcon
          style={{ marginBottom: 16 }}
          message={error.message}
          description={
            error.code === 'DIMENSION_NOT_CACHED'
              ? '该维度暂无缓存数据，请先创建导出任务，完成后即可在此浏览。'
              : undefined
          }
          action={
            error.code === 'DIMENSION_NOT_CACHED' ? (
              <Button size="small" type="primary" onClick={() => navigate('/tasks')}>
                去创建任务
              </Button>
            ) : undefined
          }
        />
      )}

      {data && (
        <div style={{ marginBottom: 12 }}>
          <Space size={12}>
            <span>
              {data.user} 的 {dimLabel(data.category, data.status)} · 共 {data.total} 条
            </span>
            <Tag color={data.fresh ? 'success' : 'default'}>{data.fresh ? '缓存有效' : '缓存已过期'}</Tag>
            <span style={{ color: '#888' }}>抓取于 {dayjs(data.cached_at).format('YYYY-MM-DD HH:mm')}</span>
          </Space>
        </div>
      )}

      <Table<RecordItem>
        size="small"
        rowKey={(r) => r.comment_id || r.subject_id}
        loading={loading}
        dataSource={data?.items ?? []}
        pagination={
          data
            ? {
                current: page,
                pageSize: PAGE_SIZE,
                total: data.total,
                showSizeChanger: false,
                onChange: setPage,
              }
            : false
        }
        columns={[
          {
            title: '',
            dataIndex: 'cover',
            width: 56,
            render: (v) => <Avatar shape="square" size={44} src={v} />,
          },
          {
            title: '条目',
            dataIndex: 'title',
            render: (_, r) => (
              <>
                <div>
                  <a href={r.subject_url} target="_blank" rel="noreferrer">
                    {r.title}
                  </a>
                </div>
                {r.alt_title && (
                  <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                    {r.alt_title}
                  </Typography.Text>
                )}
              </>
            ),
          },
          {
            title: '评分',
            dataIndex: 'my_rating',
            width: 130,
            render: (v) => (v ? <Rate disabled value={v} allowHalf style={{ fontSize: 14 }} /> : '—'),
          },
          { title: '日期', dataIndex: 'date', width: 100 },
          {
            title: '短评',
            dataIndex: 'comment',
            ellipsis: { showTitle: false },
            render: (v) =>
              v ? (
                <Tooltip title={v} placement="topLeft">
                  {v}
                </Tooltip>
              ) : (
                <span style={{ color: '#ccc' }}>—</span>
              ),
          },
        ]}
      />
    </PageContainer>
  );
}
