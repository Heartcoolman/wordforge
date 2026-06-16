/* @refresh reload */
import { render } from 'solid-js/web';
import App from './App';
import { initSentry } from './lib/sentry';
import './index.css';
import './styles/wf-admin.css';

// 渲染前先初始化 Sentry（DSN 缺省时自动跳过，不阻塞本地开发）
initSentry();

const root = document.getElementById('root');

if (!root) {
  throw new Error('Root element not found');
}

render(() => <App />, root);
