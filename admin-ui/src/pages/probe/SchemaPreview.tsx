import { createResource, For, Show } from 'solid-js';
import type { JSX } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { probeTelemetryApi } from '@/api/probeTelemetry';

const SCHEMA_EVENT_TYPE = 'periodic';

/** 样本 Schema 卡：取 telemetry_events 最新一条 periodic payload，暗色语法高亮渲染。
 *  无数据时（sampledAt/payload 为 null）走 Empty 兜底。 */
export function SchemaPreview() {
  const [data] = createResource(() => probeTelemetryApi.schema(SCHEMA_EVENT_TYPE));

  return (
    <Card variant="elevated" padding="md">
      <div class="pb-card-header">
        <h3>数据样本 · {SCHEMA_EVENT_TYPE}</h3>
        <span class="meta">
          <Show when={data()?.sampledAt} fallback="无样本">
            {(ts) => `采样于 ${ts().slice(0, 19).replace('T', ' ')}`}
          </Show>
        </span>
      </div>
      <p class="pb-panel-hint">下面是最新一条上报的真实样貌——能看出系统具体记了哪些信息。没有就表示这类事件暂时没人上报。</p>
      <Show
        when={!data.loading}
        fallback={<div class="min-h-[120px] grid place-items-center"><Spinner /></div>}
      >
        <Show
          when={data()?.payload != null}
          fallback={<Empty title="暂无样本" description={`telemetry_events 无 ${SCHEMA_EVENT_TYPE} 类型上报`} />}
        >
          <pre class="pb-schema">{highlight(data()!.payload, 0)}</pre>
        </Show>
      </Show>
    </Card>
  );
}

/** 递归渲染 JSON，键 .k / 字符串 .s / 数字 .n / 布尔 .b 着色（对齐设计稿 pb-schema）。 */
function highlight(value: unknown, depth: number): JSX.Element {
  const pad = '  '.repeat(depth);
  const padIn = '  '.repeat(depth + 1);

  if (value === null) return <span class="n">null</span>;
  if (typeof value === 'string') return <span class="s">"{value}"</span>;
  if (typeof value === 'number') return <span class="n">{String(value)}</span>;
  if (typeof value === 'boolean') return <span class="b">{String(value)}</span>;

  if (Array.isArray(value)) {
    if (value.length === 0) return <>[]</>;
    return (
      <>
        {'[\n'}
        <For each={value}>
          {(item, i) => (
            <>{padIn}{highlight(item, depth + 1)}{i() < value.length - 1 ? ',' : ''}{'\n'}</>
          )}
        </For>
        {pad}{']'}
      </>
    );
  }

  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) return <>{'{}'}</>;
    return (
      <>
        {'{\n'}
        <For each={entries}>
          {([k, v], i) => (
            <>{padIn}<span class="k">"{k}"</span>: {highlight(v, depth + 1)}{i() < entries.length - 1 ? ',' : ''}{'\n'}</>
          )}
        </For>
        {pad}{'}'}
      </>
    );
  }

  return <>{String(value)}</>;
}
