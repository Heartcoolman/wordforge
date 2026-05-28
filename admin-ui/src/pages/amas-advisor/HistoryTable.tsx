import { createResource, createSignal, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Table } from '@/components/ui/Table';
import { Modal } from '@/components/ui/Modal';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { adminApi, type AmasSuggestion, type AmasSuggestionStatus } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { formatMoney } from '@/utils/formatters';

const PAGE_SIZE = 50;

const STATUS_LABEL: Record<AmasSuggestionStatus, string> = {
  pending: '待审批', approved: '已批准', rejected: '已拒绝',
  superseded: '已被覆盖', expired: '已过期', auto_applied: '自动应用',
};
const STATUS_VARIANT: Record<AmasSuggestionStatus, 'default' | 'success' | 'error' | 'warning' | 'info'> = {
  pending: 'warning', approved: 'success', rejected: 'error',
  superseded: 'default', expired: 'default', auto_applied: 'info',
};

function fmtTime(iso: string): string {
  try { return new Date(iso).toLocaleString('zh-CN', { hour12: false }); } catch { return iso; }
}

export function HistoryTable() {
  const [q, setQ] = createSignal('');
  const [applied, setApplied] = createSignal<{ q: string; offset: number }>({ q: '', offset: 0 });
  const [viewTarget, setViewTarget] = createSignal<AmasSuggestion | null>(null);
  const [rbTarget, setRbTarget] = createSignal<AmasSuggestion | null>(null);
  const [rbBusy, setRbBusy] = createSignal(false);
  const [exporting, setExporting] = createSignal(false);

  const [rows, { refetch }] = createResource(
    applied,
    (a) => adminApi.amasListSuggestions(undefined, PAGE_SIZE, a.offset, a.q || undefined),
  );

  function search() {
    setApplied({ q: q().trim(), offset: 0 });
  }
  function prevPage() {
    setApplied((a) => ({ ...a, offset: Math.max(0, a.offset - PAGE_SIZE) }));
  }
  function nextPage() {
    setApplied((a) => ({ ...a, offset: a.offset + PAGE_SIZE }));
  }

  async function confirmRollback() {
    const t = rbTarget();
    if (!t) return;
    setRbBusy(true);
    try {
      const r = await adminApi.amasRollbackSuggestion(t.id);
      uiStore.toast.success('已回滚', `恢复到 ${r.versionHash.slice(0, 10)}`);
      setRbTarget(null);
      void refetch();
    } catch (e) {
      uiStore.toast.error('回滚失败', e instanceof Error ? e.message : '');
    } finally {
      setRbBusy(false);
    }
  }

  async function exportCsv() {
    setExporting(true);
    try {
      const csv = await adminApi.amasExportSuggestionsCsv(undefined, applied().q || undefined);
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `amas-suggestions-${Date.now()}.csv`;
      a.click();
      URL.revokeObjectURL(url);
      uiStore.toast.success('已导出 CSV');
    } catch (e) {
      uiStore.toast.error('导出失败', e instanceof Error ? e.message : '');
    } finally {
      setExporting(false);
    }
  }

  return (
    <Card variant="elevated">
      <div class="flex items-center justify-between gap-3 mb-3 flex-wrap">
        <h2 class="text-headline text-content">建议历史</h2>
        <div class="flex items-center gap-2">
          <input
            class="h-9 px-3 rounded-lg text-sm bg-surface text-content border border-border-hairline focus-ring-soft focus:border-accent"
            placeholder="搜索参数 / rationale…"
            value={q()}
            onInput={(e) => setQ(e.currentTarget.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') search(); }}
          />
          <Button size="sm" variant="outline" onClick={search}>搜索</Button>
          <Button size="sm" variant="secondary" loading={exporting()} onClick={exportCsv}>导出 CSV</Button>
        </div>
      </div>

      <Table<AmasSuggestion>
        data={rows() ?? []}
        loading={rows.loading}
        emptyText="尚无历史建议"
        aria-label="AMAS 建议历史"
        columns={[
          { key: 'status', title: '状态', render: (r) => <Badge variant={STATUS_VARIANT[r.status]} size="sm">{STATUS_LABEL[r.status]}</Badge> },
          { key: 'createdAt', title: '时间', render: (r) => <span class="text-xs tabular-nums text-content-tertiary">{fmtTime(r.createdAt)}</span> },
          { key: 'basedOnVersionHash', title: '基于版本', render: (r) => <span class="font-mono text-xs">{r.basedOnVersionHash.slice(0, 10)}</span> },
          { key: 'rationale', title: '理由', render: (r) => <span class="text-xs text-content truncate block max-w-[20rem]" title={r.rationale}>{r.rationale}</span> },
          { key: 'costUsd', title: '成本', render: (r) => <span class="text-xs tabular-nums">{r.costUsd != null ? formatMoney(r.costUsd, 4) : '—'}</span> },
          { key: 'decidedBy', title: '决策人', render: (r) => <span class="text-xs text-content-tertiary">{r.decidedBy ?? '—'}</span> },
          {
            key: '_ops', title: '操作', render: (r) => (
              <div class="flex gap-1">
                <Button size="xs" variant="ghost" onClick={() => setViewTarget(r)}>查看</Button>
                <Show when={r.status === 'approved' || r.status === 'auto_applied'}>
                  <Button size="xs" variant="ghost" onClick={() => setRbTarget(r)}>回滚</Button>
                </Show>
              </div>
            ),
          },
        ]}
      />

      <div class="flex items-center justify-between mt-3">
        <span class="text-xs text-content-tertiary">offset {applied().offset}</span>
        <div class="flex gap-2">
          <Button size="sm" variant="ghost" disabled={applied().offset === 0} onClick={prevPage}>上一页</Button>
          <Button size="sm" variant="ghost" disabled={(rows() ?? []).length < PAGE_SIZE} onClick={nextPage}>下一页</Button>
        </div>
      </div>

      {/* 查看详情 */}
      <Modal open={!!viewTarget()} onClose={() => setViewTarget(null)} title="建议详情" size="lg">
        <Show when={viewTarget()}>
          {(t) => (
            <div class="space-y-3 text-sm">
              <p class="text-content">{t().rationale}</p>
              <pre class="p-2 bg-surface-secondary rounded text-[11px] overflow-auto font-mono max-h-64">
                {JSON.stringify(t().patchJson, null, 2)}
              </pre>
            </div>
          )}
        </Show>
      </Modal>

      {/* 回滚确认 */}
      <ConfirmDialog
        open={!!rbTarget()}
        title="确认回滚该建议"
        message={<>将基于版本链 restore 回滚到 <span class="font-mono">{rbTarget()?.basedOnVersionHash.slice(0, 10)}</span> 的父版本。</>}
        confirmText="确认回滚"
        variant="warning"
        loading={rbBusy()}
        onConfirm={confirmRollback}
        onCancel={() => setRbTarget(null)}
      />
    </Card>
  );
}
