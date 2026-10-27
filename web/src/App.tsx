import { Popover, Button, Form, Input, App as AntApp } from 'antd';
import { SettingOutlined } from '@ant-design/icons';
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
import { getApiBase, getToken, setApiBase, setToken } from './api';

/** 右上角连接设置：API 地址 + Bearer Token */
function ConnectionSettings() {
  const { message } = AntApp.useApp();
  const [form] = Form.useForm<{ apiBase: string; token: string }>();
  const save = (v: { apiBase: string; token: string }) => {
    setApiBase(v.apiBase.trim().replace(/\/$/, ''));
    setToken(v.token.trim());
    message.success('已保存，正在刷新…');
    window.location.reload();
  };
  return (
    <div style={{ width: 300, padding: 4 }}>
      <Form form={form} layout="vertical" onFinish={save} initialValues={{ apiBase: getApiBase(), token: getToken() }}>
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
  return (
    <ProLayout
      title="豆瓣导出后台"
      logo={false}
      layout="side"
      fixSiderbar
      location={{ pathname: location.pathname }}
      route={{
        path: '/',
        routes: [
          { path: '/', name: '概览' },
          { path: '/tasks', name: '导出任务' },
          { path: '/marks', name: '数据浏览' },
        ],
      }}
      menuItemRender={(item, dom) => (
        <div onClick={() => item.path && navigate(item.path)}>{dom}</div>
      )}
      actionsRender={() => [
        <Popover key="conn" trigger="click" placement="bottomRight" content={<ConnectionSettings />}>
          <Button type="text" icon={<SettingOutlined />}>
            连接
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
