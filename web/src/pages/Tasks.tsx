import { useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { App as AntApp, Popconfirm, theme } from 'antd';
import { ActionType, PageContainer, ProTable, type ProColumns } from '@ant-design/pro-components';
import dayjs from 'dayjs';
import type { TaskSummary } from '../types';
import { api, getUser } from '../api';
import { TASK_STATUS_META, TaskStatusTag } from '../labels';
import CreateTaskModal from '../components/CreateTaskModal';
import TaskDetailDrawer from '../components/TaskDetailDrawer';

const ACTIVE = ['queued', 'running'];

export default function Tasks() {
  const actionRef = useRef<ActionType>();
  const { token } = theme.useToken();
  const [openTaskId, setOpenTaskId] = useState<string | null>(null);
  const [hasActive, setHasActive] = useState(false);
  const { message } = AntApp.useApp();
  const [searchParams] = useSearchParams();

  // 支持从概览跳转 /tasks?task=xxx 直达详情
  useEffect(() => {
    const task = searchParams.get('task');
    if (task) setOpenTaskId(task);
  }, [searchParams]);

  const columns: ProColumns<TaskSummary>[] = [
    {
      title: '任务 ID',
      dataIndex: 'task_id',
      copyable: true,
      ellipsis: true,
      width: 230,
      search: false,
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 110,
      valueEnum: Object.fromEntries(
        Object.entries(TASK_STATUS_META).map(([k, v]) => [k, { text: v.label }]),
      ),
      render: (_, r) => <TaskStatusTag status={r.status} />,
    },
    { title: '条目', dataIndex: 'items_count', width: 90, search: false },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      width: 170,
      search: false,
      render: (_, r) => dayjs(r.created_at).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '操作',
      valueType: 'option',
      width: 120,
      render: (_, r) => [
        <a key="detail" onClick={() => setOpenTaskId(r.task_id)}>
          详情
        </a>,
        <Popconfirm
          key="delete"
          title="删除任务记录？已入库数据不受影响"
          description="缓存数据不受影响；运行中任务会先取消"
          onConfirm={async () => {
            try {
              await api.deleteTask(r.task_id);
              message.success('已删除');
              actionRef.current?.reload();
            } catch (e) {
              message.error((e as Error).message);
            }
          }}
        >
          <a style={{ color: token.colorError }}>删除</a>
        </Popconfirm>,
      ],
    },
  ];

  return (
    <PageContainer title="同步任务">
      <ProTable<TaskSummary>
        rowKey="task_id"
        actionRef={actionRef}
        columns={columns}
        search={{ labelWidth: 'auto' }}
        polling={hasActive ? 2000 : undefined}
        options={{ reload: true, density: false, setting: false }}
        locale={{ emptyText: '暂无任务，点击右上角「新建同步」创建' }}
        request={async (params) => {
          const pageSize = params.pageSize ?? 20;
          const need = (params.current ?? 1) * pageSize;
          const tasks: TaskSummary[] = [];
          let cursor: number | undefined;
          let hasMore = false;
          // 游标按创建时间倒序翻页：每次拉满 100 条，直到覆盖当前页所需
          while (tasks.length < need) {
            const res = await api.listTasks({
              // 单用户站点：只看「连接」里设置的用户的任务（未设置时过渡展示全部）
              user: getUser() || undefined,
              status: params.status || undefined,
              limit: 100,
              cursor,
            });
            tasks.push(...res.tasks);
            if (res.next_cursor == null || res.tasks.length === 0) {
              hasMore = false;
              break;
            }
            cursor = res.next_cursor;
            hasMore = true;
          }
          setHasActive(tasks.some((t) => ACTIVE.includes(t.status)));
          return {
            data: tasks,
            total: tasks.length + (hasMore ? pageSize : 0),
            success: true,
          };
        }}
        toolBarRender={() => [
          <CreateTaskModal
            key="create"
            onCreated={(taskId) => {
              actionRef.current?.reload();
              setOpenTaskId(taskId);
            }}
          />,
        ]}
      />
      <TaskDetailDrawer taskId={openTaskId} onClose={() => setOpenTaskId(null)} />
    </PageContainer>
  );
}
