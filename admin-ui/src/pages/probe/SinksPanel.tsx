import { createResource, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import { compact } from './util';

/** 真实 SQLite sinks 卡：5 张表（telemetry_events/summaries/learning_records/sessions/
 *  engine_monitoring_events）。retentionDays 恒 null → 显示"永久/不限"；lagSecs > 300 标 late。 */
export function SinksPanel() {
  const [data] = createResource(() => probeTelemetryApi.sinks());

  return (
    <Card variant="elevated" padding="md">
      <div class="pb-card-header">
        <h3>写入目标（SQLite 表）</h3>
        <span class="meta">
          <Show when={data()} fallback="加载中">{(d) => `${d().sinks.length} sinks`}</Show>
        </span>
      </div>
      <Show
        when={!data.loading}
        fallback={<div class="min-h-[120px] grid place-items-center"><Spinner /></div>}
      >
        <Show
          when={(data()?.sinks.length ?? 0) > 0}
          fallback={<Empty title="暂无 sink" description="sinks_status 返回空" />}
        >
          <div>
            <For each={data()!.sinks}>
              {(sink) => {
                const late = sink.lagSecs > 300;
                const initials = sink.id.slice(0, 2).toUpperCase();
                return (
                  <div class={`pb-sink${late ? ' is-late' : ''}`}>
                    <div class="logo">{initials}</div>
                    <div class="body">
                      <strong>{sink.label} · {sink.id}</strong>
                      <div class="meta">
                        {compact(sink.rowCount)} 行 · retention={sink.retentionDays == null ? '永久/不限' : `${sink.retentionDays}d`}
                      </div>
                    </div>
                    <span class="lag">
                      {sink.lastWriteTs ? `+${sink.lagSecs}s` : '无写入'}
                    </span>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </Show>
    </Card>
  );
}
