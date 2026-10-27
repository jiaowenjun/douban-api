import { App as AntApp, Button } from 'antd';
import { PlusOutlined } from '@ant-design/icons';
import { ModalForm, ProFormSelect, ProFormSwitch, ProFormText } from '@ant-design/pro-components';
import { api } from '../api';
import { CATEGORY_OPTIONS, STATUS_OPTIONS } from '../labels';

export default function CreateTaskModal({ onCreated }: { onCreated: (taskId: string) => void }) {
  const { message } = AntApp.useApp();
  return (
    <ModalForm
      title="新建同步任务"
      width={520}
      trigger={
        <Button type="primary" icon={<PlusOutlined />}>
          新建同步
        </Button>
      }
      modalProps={{ destroyOnClose: true }}
      initialValues={{
        categories: ['movie', 'book', 'music'],
        statuses: ['collect'],
        refresh: false,
      }}
      onFinish={async (values) => {
        try {
          const res = await api.createTask({
            user: values.user.trim(),
            categories: values.categories,
            statuses: values.statuses,
            refresh: values.refresh ?? false,
          });
          message.success(
            `任务已创建（缓存命中 ${res.cached_dimensions ?? 0} 个维度，新建 ${res.created_dimensions ?? 0} 个维度）`,
          );
          onCreated(res.task_id);
          return true;
        } catch (e) {
          message.error((e as Error).message);
          return false;
        }
      }}
    >
      <ProFormText
        name="user"
        label="豆瓣用户"
        rules={[{ required: true, message: '请输入用户 ID 或个人主页链接' }]}
        placeholder="如 wenjun30 或 https://www.douban.com/people/wenjun30/"
        extra="同步约需 1–3 分钟（受豆瓣限速约束），完成后数据入库并可在数据浏览页查看"
      />
      <ProFormSelect
        name="categories"
        label="类别"
        mode="multiple"
        options={CATEGORY_OPTIONS}
        rules={[{ required: true, message: '至少选择一个类别' }]}
      />
      <ProFormSelect
        name="statuses"
        label="标记状态"
        mode="multiple"
        options={STATUS_OPTIONS}
        rules={[{ required: true, message: '至少选择一个状态' }]}
      />
      <ProFormSwitch name="refresh" label="强制刷新缓存" extra="忽略未过期缓存，重新回源抓取" />
    </ModalForm>
  );
}
