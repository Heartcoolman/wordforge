import { createSignal, Show, For, onMount, createMemo } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';
import type { FeedbackItem } from '@/types/admin';

/**
 * Admin 反馈中心：对接 GET /api/admin/feedback?page=&perPage=。
 * cross-validator P1-1：后端 admin/feedback.rs 路由就绪但前端零对接，
 * 用户提交的反馈无法在 admin 后台查看。
 */
export default function FeedbackPage() {
  const [items, setItems] = createSignal<FeedbackItem[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  const [perPage] = createSignal(20);
  const [loading, setLoading] = createSignal(true);
  const [err, setErr] = createSignal('');

  const totalPages = createMemo(() => Math.max(1, Math.ceil(total() / perPage())));

  async function load(targetPage: number) {
    setLoading(true);
    setErr('');
    try {
      const resp = await adminApi.listFeedback({ page: targetPage, perPage: perPage() });
      setItems(resp.items);
      setTotal(resp.total);
      setPage(resp.page);
    } catch (e) {
      setErr(e instanceof Error ? e.message : '加载失败');
    } finally {
      setLoading(false);
    }
  }

  onMount(() => load(1));

  return (
    <div class="space-y-4">
      <header class="flex items-center justify-between">
        <div>
          <h1 class="text-2xl font-semibold text-content">反馈中心</h1>
          <p class="text-sm text-content-tertiary mt-1">用户通过 /api/feedback 提交的内容，按时间倒序展示。</p>
        </div>
        <Badge>共 {total()} 条</Badge>
      </header>

      <Card>
        <Show when={!loading()} fallback={<div class="py-8 flex justify-center"><Spinner /></div>}>
          <Show when={!err()} fallback={<div class="py-8 text-error text-sm">{err()}</div>}>
            <Show when={items().length > 0} fallback={<Empty title="暂无反馈" />}>
              <div class="divide-y divide-border">
                <For each={items()}>
                  {(it) => (
                    <article class="py-3">
                      <div class="flex items-center justify-between gap-2 text-xs text-content-tertiary">
                        <div class="flex items-center gap-2">
                          <Show when={it.category}>
                            <Badge variant="info">{it.category}</Badge>
                          </Show>
                          <Show when={it.route}>
                            <span class="font-mono">{it.route}</span>
                          </Show>
                        </div>
                        <time>{new Date(it.createdAt).toLocaleString('zh-CN')}</time>
                      </div>
                      <p class="mt-1.5 text-sm text-content whitespace-pre-wrap break-words">{it.body}</p>
                      <p class="mt-1 text-xs text-content-tertiary">用户 ID：<span class="font-mono">{it.userId}</span></p>
                    </article>
                  )}
                </For>
              </div>

              <Show when={totalPages() > 1}>
                <nav class="flex items-center justify-end gap-2 pt-3 text-sm">
                  <button class="btn btn-ghost btn-sm" disabled={page() <= 1} onClick={() => load(page() - 1)}>上一页</button>
                  <span class="text-content-tertiary">第 {page()} / {totalPages()} 页</span>
                  <button class="btn btn-ghost btn-sm" disabled={page() >= totalPages()} onClick={() => load(page() + 1)}>下一页</button>
                </nav>
              </Show>
            </Show>
          </Show>
        </Show>
      </Card>
    </div>
  );
}
