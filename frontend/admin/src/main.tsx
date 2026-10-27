import ReactDOM from 'react-dom/client';
import { App as AntApp } from 'antd';
import dayjs from 'dayjs';
import 'dayjs/locale/zh-cn';
import App from './App';
import { AppThemeProvider } from './theme';
import './index.css';

dayjs.locale('zh-cn');

ReactDOM.createRoot(document.getElementById('root')!).render(
  <AppThemeProvider>
    <AntApp>
      <App />
    </AntApp>
  </AppThemeProvider>,
);
