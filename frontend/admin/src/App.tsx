import { useEffect, useState } from 'react';
import { Popover, Button, Form, Input, App as AntApp } from 'antd';
import { SettingOutlined, UserOutlined } from '@ant-design/icons';
import { ProLayout } from '@ant-design/pro-components';
import {
  BrowserRouter,
  Navigate,
  Outlet,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router-dom';
import Overview from './pages/Overview';
import Tasks from './pages/Tasks';
import Marks from './pages/Marks';
import { getApiBase, getToken, getUser, parseUserInput, setApiBase, setToken, setUser } from './api';

/** 右上角连接设置：豆瓣用户 ID（单用户站点的身份，只在这里填一次）+ API 地址 + Bearer Token */
function ConnectionSettings() {
  const { message } = AntApp.useApp();
  const [form] = Form.useForm<{ user: string; apiBase: string; token: string }>();
  const save = (v: { user: string; apiBase: string; token: string }) => {
    const uid = parseUserInput(v.user);
    if (!uid) {
      message.error('用户 ID 不合法：仅限字母、数字、.、-、_，或粘贴豆瓣个人主页链接');
      return;
    }
    setApiBase(v.apiBase.trim().replace(/\/$/, ''));
    setToken(v.token.trim());
    setUser(uid);
    message.success('已保存，正在刷新…');
    window.location.reload();
  };
  return (
    <div style={{ width: 320, padding: 4 }}>
      <Form form={form} layout="vertical" onFinish={save} initialValues={{ user: getUser(), apiBase: getApiBase(), token: getToken() }}>
        <Form.Item
          name="user"
          label="豆瓣用户 ID"
          rules={[{ required: true, message: '请输入用户 ID 或个人主页链接' }]}
          extra="整站以此用户身份同步与浏览数据；粘贴主页链接会自动提取 ID"
        >
          <Input placeholder="如 wenjun30 或 https://www.douban.com/people/wenjun30/" />
        </Form.Item>
        <Form.Item
          name="apiBase"
          label="API 地址"
          rules={[{ required: true }]}
          extra="同源部署保持 /api/v1；跨域填完整地址并需服务器开启 CORS"
        >
          <Input placeholder="/api/v1" />
        </Form.Item>
        <Form.Item name="token" label="Bearer Token" extra="服务器未设置 EXPORT_SERVER_TOKEN 时留空">
          <Input.Password placeholder="留空表示无鉴权" />
        </Form.Item>
        <Button type="primary" htmlType="submit" block>
          保存并刷新
        </Button>
      </Form>
    </div>
  );
}

function Layout() {
  const location = useLocation();
  const navigate = useNavigate();
  const [connOpen, setConnOpen] = useState(false);
  const user = getUser();

  // 首次访问未设置用户名时，主动打开连接弹框引导填写
  useEffect(() => {
    if (!getUser()) setConnOpen(true);
  }, []);

  return (
    <ProLayout
      title="豆瓣数据后台"
      logo={false}
      layout="side"
      fixSiderbar
      location={{ pathname: location.pathname }}
      route={{
        path: '/',
        routes: [
          { path: '/', name: '概览' },
          { path: '/tasks', name: '同步任务' },
          { path: '/marks', name: '数据浏览' },
        ],
      }}
      menuItemRender={(item, dom) => (
        <div onClick={() => item.path && navigate(item.path)}>{dom}</div>
      )}
      actionsRender={() => [
        <Popover
          key="conn"
          trigger="click"
          placement="bottomRight"
          open={connOpen}
          onOpenChange={setConnOpen}
          content={<ConnectionSettings />}
        >
          <Button type="text" icon={user ? <UserOutlined /> : <SettingOutlined />}>
            {user ? `@${user}` : '连接'}
          </Button>
        </Popover>,
      ]}
    >
      <Outlet />
    </ProLayout>
  );
}

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<Overview />} />
          <Route path="tasks" element={<Tasks />} />
          <Route path="marks" element={<Marks />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
