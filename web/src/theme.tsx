import { useEffect, useState, type ReactNode } from 'react';
import { ConfigProvider, theme } from 'antd';
import zhCN from 'antd/locale/zh_CN';

/** 跟随系统深浅色设置（prefers-color-scheme），系统切换时实时更新 */
export function useSystemDarkTheme(): boolean {
  const [dark, setDark] = useState(
    () => window.matchMedia('(prefers-color-scheme: dark)').matches,
  );
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (e: MediaQueryListEvent) => setDark(e.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);
  return dark;
}

/** 同步 <html>/<body>：colorScheme 让原生滚动条、表单控件随主题，底色避免暗色下露白 */
function ThemeSync({ dark }: { dark: boolean }) {
  const { token } = theme.useToken();
  useEffect(() => {
    document.documentElement.style.colorScheme = dark ? 'dark' : 'light';
    document.body.style.backgroundColor = token.colorBgLayout;
  }, [dark, token.colorBgLayout]);
  return null;
}

export function AppThemeProvider({ children }: { children: ReactNode }) {
  const dark = useSystemDarkTheme();
  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        algorithm: dark ? theme.darkAlgorithm : theme.defaultAlgorithm,
        token: { colorPrimary: '#007722', borderRadius: 6 },
      }}
    >
      <ThemeSync dark={dark} />
      {children}
    </ConfigProvider>
  );
}
