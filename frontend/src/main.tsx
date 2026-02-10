/* @refresh reload */
import { render } from 'solid-js/web';
import { QueryClientProvider } from '@tanstack/solid-query';
import { queryClient } from '@/lib/queryClient';
import App from './App';
import './index.css';

const root = document.getElementById('root');

if (!root) {
  throw new Error('Root element not found');
}

render(
  () => (
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  ),
  root,
);
