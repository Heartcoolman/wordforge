import { ErrorBoundary as SolidErrorBoundary, type ParentProps } from 'solid-js';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';

export function AppErrorBoundary(props: ParentProps) {
  return (
    <SolidErrorBoundary
      fallback={(err, reset) => {
        console.error('[AppErrorBoundary]', err);
        return (
        <div class="min-h-[60vh] flex items-center justify-center p-4">
          <Card variant="elevated" class="max-w-md w-full text-center">
            <div class="w-14 h-14 mx-auto mb-4 rounded-full bg-error-light flex items-center justify-center">
              <svg class="w-7 h-7 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.034 16.5c-.77.833.192 2.5 1.732 2.5z" />
              </svg>
            </div>
            <h2 class="text-lg font-semibold text-content mb-2">出错了</h2>
            <p class="text-sm text-content-secondary mb-4">
              {/* import.meta.env.DEV 由 Vite 在编译时注入，生产构建中会被静态替换为 false */}
              {import.meta.env.DEV
                ? (err instanceof Error ? err.message : String(err))
                : '页面出现错误，请刷新重试'}
            </p>
            <div class="flex gap-3 justify-center">
              <Button variant="outline" onClick={() => window.location.href = '/'}>返回首页</Button>
              <Button onClick={reset}>重试</Button>
            </div>
          </Card>
        </div>
        );
      }}
    >
      {props.children}
    </SolidErrorBoundary>
  );
}
