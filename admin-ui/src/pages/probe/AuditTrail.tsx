import { createResource, For, Show } from 'solid-js';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import type { AuditRow } from '@/types/probeTelemetry';
import { hms } from './util';

const AUDIT_LIMIT = 20;

const ACTION_LABEL: Record<string, string> = { add: 'ADD', mod: 'MODIFY', pause: 'PAUSE', del: 'DELETE' };

/** 审计区：采样规则改动历史（add/mod/pause/del），最近 20 条。空表走 Empty。 */
export function AuditTrail() {
  const [data] = createResource(() => probeTelemetryApi.audit(AUDIT_LIMIT));

  return (
    <Show
      when={!data.loading}
      fallback={<div class="min-h-[100px] grid place-items-center"><Spinner /></div>}
    >
      <Show
        when={(data()?.rows.length ?? 0) > 0}
        fallback={<Empty title="暂无改动记录" description="probe_sampling_audit 表为空" />}
      >
        <div style={{ padding: '6px 18px 14px' }}>
          <For each={data()!.rows}>{(row) => <AuditRowView row={row} />}</For>
        </div>
      </Show>
    </Show>
  );
}

function AuditRowView(props: { row: AuditRow }) {
  const r = () => props.row;
  const badge = () => (['add', 'mod', 'pause', 'del'].includes(r().action) ? r().action : 'mod');
  const who = () => r().adminId ?? '系统';

  return (
    <div class="pb-audit-row">
      <span class="ts">{hms(r().ts)}</span>
      <span class="who">
        <b style={{ color: 'var(--content)' }}>{who()}</b> {actionVerb(r().action)}{' '}
        <code>{r().eventType}</code>
        <Show when={r().oldValue && r().newValue}>
          {' '}<span style={{ color: 'var(--content-tertiary)' }}>{r().oldValue}</span>
          {' → '}<span style={{ color: 'var(--accent)' }}>{r().newValue}</span>
        </Show>
      </span>
      <span class={`badge-mini ${badge()}`}>{ACTION_LABEL[r().action] ?? r().action.toUpperCase()}</span>
    </div>
  );
}

function actionVerb(action: string): string {
  switch (action) {
    case 'add': return '新增';
    case 'mod': return '修改';
    case 'pause': return '暂停/恢复';
    case 'del': return '删除';
    default: return '改动';
  }
}
