import { createSignal, Show, For, onMount, createMemo } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Badge } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Pagination } from '@/components/ui/Pagination';
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
  // 首次加载完成后翻页不再 stagger，避免瀑布
  const [firstLoadDone, setFirstLoadDone] = createSignal(false);

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
      setFirstLoadDone(true);
    }
  }

  onMount(() => load(1));

  return (
    <div class="space-y-4">
      <header class="flex items-center justify-between pb-2 border-b border-border-hairline">
        <div>
          <h1 class="text-title text-content">反馈中心</h1>
          <p class="text-caption mt-1">用户通过 /api/feedback 提交的内容，按时间倒序展示</p>
        </div>
        {/* loading 期间隐藏 Badge，避免显示"共 0 条" */}
        <Show when={!loading()}>
          <Badge variant="accent" size="md">共 {total()} 条</Badge>
        </Show>
      </header>

      <Card padding="none">
        <Show when={!loading()} fallback={<div class="py-12 flex justify-center"><Spinner size="lg" /></div>}>
          <Show
            when={!err()}
            fallback={
              <div class="py-12 px-4 flex flex-col items-center gap-3 text-center">
                <p class="text-error text-sm">{err()}</p>
                <Button variant="ghost" size="sm" onClick={() => load(page())}>
                  重试
                </Button>
              </div>
            }
          >
            <Show when={items().length > 0} fallback={<Empty title="暂无反馈" description="还没有用户提交过反馈" />}>
              <ul class="divide-y divide-border-hairline">
                <For each={items()}>
                  {(it, idx) => (
                    <li
                      class="group p-4 transition-colors duration-fast ease-out-expo hover:bg-accent-light/40 animate-fade-in-up"
                      style={
                        firstLoadDone()
                          ? { 'animation-delay': '0ms', 'animation-fill-mode': 'backwards' }
                          : { 'animation-delay': `${Math.min(idx() * 40, 320)}ms`, 'animation-fill-mode': 'backwards' }
                      }
                    >
                      <div class="flex items-center justify-between gap-2 text-caption mb-2">
                        <div class="flex items-center gap-2">
                          <Show when={it.category}>
                            <Badge variant="info" dot>{it.category}</Badge>
                          </Show>
                          <Show when={it.route}>
                            <span class="font-mono text-content-tertiary">{it.route}</span>
                          </Show>
                        </div>
                        <time class="tabular-nums">{new Date(it.createdAt).toLocaleString('zh-CN')}</time>
                      </div>
                      <p class="text-sm text-content whitespace-pre-wrap break-words leading-relaxed">{it.body}</p>
                      <p class="mt-2 text-caption">用户 ID：<span class="font-mono">{it.userId}</span></p>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </Show>
        </Show>

        {/* 分页器移到 Show 外：err / empty 状态也保留，方便重试后翻页 */}
        <Show when={!loading() && totalPages() > 1}>
          <div class="flex items-center justify-between gap-2 p-3 border-t border-border-hairline">
            <span class="text-caption">第 {page()} / {totalPages()} 页</span>
            <Pagination page={page()} total={total()} pageSize={perPage()} onChange={load} />
          </div>
        </Show>
      </Card>
    </div>
  );
}
