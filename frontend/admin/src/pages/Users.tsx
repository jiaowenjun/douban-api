import { useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { App as AntApp, Badge, Popconfirm, theme } from 'antd';
import { ActionType, PageContainer, ProTable, type ProColumns } from '@ant-design/pro-components';
import dayjs from 'dayjs';
import type { UserSummary } from '../types';
import { api } from '../api';
import { UserCell } from '../user-profile';
import CreateTaskModal from '../components/CreateTaskModal';

const fmtTime = (v: string | null) => (v ? dayjs(v).format('MM-DD HH:mm') : '—');

/** 全用户管理：四表并集的用户列表（users ∪ records ∪ subscriptions ∪ tasks），
 *  每行直达该用户的数据浏览 / 任务筛选 / 创建同步 / 订阅开关 */
export default function Users() {
  const actionRef = useRef<ActionType>();
  const navigate = useNavigate();
  const { message } = AntApp.useApp();
  const { token } = theme.useToken();

  const toggleSubscription = async (r: UserSummary) => {
    try {
      if (r.subscribed) {
        await api.deleteSubscription(r.uid);
        message.success(`已取消订阅 ${r.username || r.uid}`);
      } else {
        await api.createSubscription(r.uid, {});
        message.success(`已订阅 ${r.username || r.uid}（增量 10 分钟内生效，全量按周期自动刷新）`);
      }
      actionRef.current?.reload();
    } catch (e) {
      message.error((e as Error).message);
    }
  };

  const columns: ProColumns<UserSummary>[] = [
    {
      title: '用户',
      dataIndex: 'uid',
      render: (_, r) => <UserCell uid={r.uid} size={24} />,
    },
    { title: '记录', dataIndex: 'records', width: 90 },
    { title: '维度', dataIndex: 'dimensions', width: 80 },
    {
      title: '订阅',
      dataIndex: 'subscribed',
      width: 90,
      render: (_, r) =>
        r.subscribed ? <Badge status="processing" text="已订阅" /> : <Badge status="default" text="未订阅" />,
    },
    { title: '最近任务', dataIndex: 'last_task_at', width: 110, render: (_, r) => fmtTime(r.last_task_at) },
    { title: '最近抓取', dataIndex: 'last_sync_at', width: 110, render: (_, r) => fmtTime(r.last_sync_at) },
    {
      title: '资料更新',
      dataIndex: 'profile_updated_at',
      width: 110,
      render: (_, r) => fmtTime(r.profile_updated_at),
    },
    {
      title: '操作',
      valueType: 'option',
      width: 230,
      render: (_, r) => [
        <a key="marks" onClick={() => navigate(`/marks?uid=${encodeURIComponent(r.uid)}`)}>
          数据
        </a>,
        <a key="tasks" onClick={() => navigate(`/tasks?user=${encodeURIComponent(r.uid)}`)}>
          任务
        </a>,
        <CreateTaskModal
          key="sync"
          presetUser={r.uid}
          buttonType="link"
          onCreated={(taskId) => {
            navigate(`/tasks?task=${taskId}`);
          }}
        />,
        r.subscribed ? (
          <Popconfirm key="unsub" title={`取消订阅 ${r.username || r.uid}？`} onConfirm={() => toggleSubscription(r)}>
            <a style={{ color: token.colorError }}>取消订阅</a>
          </Popconfirm>
        ) : (
          <a key="sub" onClick={() => toggleSubscription(r)}>
            订阅
          </a>
        ),
      ],
    },
  ];

  return (
    <PageContainer title="用户" subTitle="来源：已入库资料 ∪ 已有数据 ∪ 已订阅 ∪ 建过任务，按记录数降序">
      <ProTable<UserSummary>
        rowKey="uid"
        actionRef={actionRef}
        columns={columns}
        search={false}
        options={{ reload: true, density: false, setting: false }}
        locale={{ emptyText: '暂无用户——新建同步任务或添加订阅后即出现' }}
        request={async () => {
          // 旧版后端无此端点（404）时回退空列表，页面不炸
          let users: UserSummary[] = [];
          try {
            users = (await api.listUsers()).users;
          } catch {
            /* 保持空列表 */
          }
          return { data: users, total: users.length, success: true };
        }}
      />
    </PageContainer>
  );
}
