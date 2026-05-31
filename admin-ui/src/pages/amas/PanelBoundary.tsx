import { ErrorBoundary, type JSX } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Empty } from '@/components/ui/Empty';

/** 单面板错误边界：某面板的资源请求失败（如后端缺该端点 404）时只降级该面板，
 * 不让错误冒泡到 app 级 ErrorBoundary 而整页崩溃。 */
export function PanelBoundary(props: { children: JSX.Element }) {
  return (
    <ErrorBoundary
      fallback={(err) => (
        <Card variant="elevated" class="amas-panel">
          <Empty
            title="该面板加载失败"
            description={err?.message ? String(err.message) : String(err)}
          />
        </Card>
      )}
    >
      {props.children}
    </ErrorBoundary>
  );
}
