import { useEffect, useState } from 'react';
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
import Subjects from './pages/Subjects';
import SubjectJobs from './pages/SubjectJobs';
import { getApiBase, getToken, getUser, parseUserInput, setApiBase, setToken, setUser } from './api';
import { UserAvatar, useUserProfile } from './user-profile';

/** 微墨墨滴标：currentColor 单色标，颜色随 antd 侧栏文字色自适应明暗主题 */
function BrandLogo() {
  return (
    <svg aria-hidden="true" viewBox="0 0 512 512" width={26} height={26} style={{ display: 'block' }}>
      <defs>
        <linearGradient id="wmDropTone" x1="0.18" y1="0.08" x2="0.82" y2="0.94">
          <stop offset="0" stopColor="currentColor" stopOpacity="0.96" />
          <stop offset="0.58" stopColor="currentColor" />
          <stop offset="1" stopColor="currentColor" stopOpacity="0.82" />
        </linearGradient>
        <linearGradient id="wmGlint" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#fff" stopOpacity="0.92" />
          <stop offset="0.55" stopColor="#fff" stopOpacity="0.38" />
          <stop offset="1" stopColor="#fff" stopOpacity="0.02" />
        </linearGradient>
      </defs>
      <path
        fill="url(#wmDropTone)"
        d="M256 54C241 111 220 153 184 202C148 251 119 291 119 343C119 421 178 468 256 468C334 468 393 421 393 343C393 291 364 251 328 202C292 153 271 111 256 54Z"
      />
      <path
        fill="url(#wmGlint)"
        d="M246 112C233 157 213 192 187 231C163 267 151 295 151 323C151 341 157 354 168 361C179 368 190 357 195 340C204 309 220 279 240 247C259 217 266 181 246 112Z"
      />
    </svg>
  );
}

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
  // 已同步过时显示豆瓣昵称与头像；未同步（404）回退 @uid 与图标
  const profile = useUserProfile(user || null);

  // 首次访问未设置用户名时，主动打开连接弹框引导填写
  useEffect(() => {
    if (!getUser()) setConnOpen(true);
  }, []);

  return (
    <ProLayout
      title="微墨·豆瓣"
      logo={<BrandLogo />}
      layout="side"
      fixSiderbar
      location={{ pathname: location.pathname }}
      route={{
        path: '/',
        routes: [
          { path: '/', name: '概览' },
          { path: '/tasks', name: '同步任务' },
          { path: '/marks', name: '数据浏览' },
          { path: '/subjects', name: '条目元信息' },
          { path: '/subjects/jobs', name: '富集任务' },
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
          <Button type="text" icon={user ? <UserAvatar uid={user} size={24} /> : <SettingOutlined />}>
            {user ? profile?.username || `@${user}` : '连接'}
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
    /* basename 跟随 Vite base：生产为 /douban-admin/（子路径部署），dev 为 / 等价于无 basename */
    <BrowserRouter basename={import.meta.env.BASE_URL}>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<Overview />} />
          <Route path="tasks" element={<Tasks />} />
          <Route path="marks" element={<Marks />} />
          <Route path="subjects/jobs" element={<SubjectJobs />} />
          <Route path="subjects" element={<Subjects />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
