import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { Button } from '@/components/ui/Button';
import { Empty } from '@/components/ui/Empty';
import { uiStore } from '@/stores/ui';
import { recordsApi } from '@/api/records';
import { wordsApi } from '@/api/words';
import type { LearningRecord } from '@/types/record';
import type { Word } from '@/types/word';
import { formatDateTime, formatResponseTime } from '@/utils/formatters';

export default function HistoryPage() {
  const [records, setRecords] = createSignal<LearningRecord[]>([]);
  const [wordMap, setWordMap] = createSignal<Record<string, Word>>({});
  const [loading, setLoading] = createSignal(true);
  const [page, setPage] = createSignal(1);
  const [hasMore, setHasMore] = createSignal(true);
  const [loadingMore, setLoadingMore] = createSignal(false);
  const perPage = 30;

  async function load(append = false) {
    if (!append) setLoading(true);
    try {
      const res = await recordsApi.list({ perPage, page: page() });
      const items = res.data ?? [];
      if (!append) setRecords(items); else setRecords((prev) => [...prev, ...items]);
      setHasMore(res.page < res.totalPages);

      // Batch-load word info for new records (avoid N+1)
      const existingMap = wordMap();
      const newIds = [...new Set(items.map((r) => r.wordId).filter((id) => !existingMap[id]))];
      if (newIds.length > 0) {
        const newMap = { ...existingMap };
        const results = await Promise.allSettled(newIds.map((id) => wordsApi.get(id)));
        for (let idx = 0; idx < results.length; idx++) {
          const r = results[idx];
          if (r.status === 'fulfilled') newMap[newIds[idx]] = r.value;
        }
        setWordMap(newMap);
      }
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  onMount(() => load());

  async function loadMore() {
    if (loadingMore()) return;
    setLoadingMore(true);
    setPage((p) => p + 1);
    await load(true);
    setLoadingMore(false);
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">学习历史</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={records().length > 0} fallback={<Empty title="暂无学习记录" description="开始学习后记录将显示在这里" />}>
          <div class="space-y-2">
            <For each={records()}>
              {(record) => {
                const word = () => wordMap()[record.wordId];
                return (
                  <Card variant="outlined" padding="sm" class="flex items-center justify-between">
                    <div class="flex items-center gap-3">
                      <Badge variant={record.isCorrect ? 'success' : 'error'} size="sm">
                        {record.isCorrect ? '正确' : '错误'}
                      </Badge>
                      <div>
                        <p class="font-medium text-content">{word()?.text ?? record.wordId}</p>
                        <Show when={word()}><p class="text-xs text-content-secondary">{word()!.meaning}</p></Show>
                      </div>
                    </div>
                    <div class="text-right text-xs text-content-tertiary">
                      <p>{formatResponseTime(record.responseTimeMs)}</p>
                      <p>{formatDateTime(record.createdAt)}</p>
                    </div>
                  </Card>
                );
              }}
            </For>
          </div>
          <Show when={hasMore()}>
            <div class="text-center"><Button variant="ghost" onClick={loadMore} loading={loadingMore()}>加载更多</Button></div>
          </Show>
        </Show>
      </Show>
    </div>
  );
}
