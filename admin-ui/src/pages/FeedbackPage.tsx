import { createMemo, createSignal, For, onMount, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { HeroCard } from '@/components/ui/HeroCard';
import { Badge } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Pagination } from '@/components/ui/Pagination';
import { Drawer } from '@/components/ui/Drawer';
import { Select } from '@/components/ui/Select';
import { Table } from '@/components/ui/Table';
import { Collapsible } from '@/components/ui/Collapsible';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import type { FeedbackItem } from '@/types/admin';

/**
 * Admin 反馈中心 —— M1-G3 完整工单流转。
 *
 * 后端就绪能力:
 *  - GET /api/admin/feedback?category=&status=&page=&perPage= (分页 + 双 filter)
 *  - PATCH /api/admin/feedback/:id 更新 priority / status / assigneeAdminId / resolution
 *
 * UI 层:
 *  - HeroCard 4 项 KPI(总数 + open/in_progress/resolved/closed 分组)
 *  - status chip filter 行
 *  - Table 行点击 → 右侧 Drawer 详情 + 编辑表单 + save
 */

const STATUS_VARIANTS: Record<string, 'warning' | 'info' | 'success' | 'default'> = {
  open: 'warning',
  in_progress: 'info',
  resolved: 'success',
  closed: 'default',
};

const STATUS_LABEL: Record<string, string> = {
  open: '待处理',
  in_progress: '处理中',
  resolved: '已解决',
  closed: '已关闭',
};

const PRIORITY_VARIANT: Record<string, 'error' | 'warning' | 'info' | 'default'> = {
  urgent: 'error',
  high: 'warning',
  normal: 'info',
  low: 'default',
};

const PRIORITY_LABEL: Record<string, string> = {
  urgent: '紧急',
  high: '高',
  normal: '正常',
  low: '低',
};

type StatusFilter = '' | 'open' | 'in_progress' | 'resolved' | 'closed';

export default function FeedbackPage() {
  const [items, setItems] = createSignal<FeedbackItem[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  const [perPage] = createSignal(20);
  const [loading, setLoading] = createSignal(true);
  const [err, setErr] = createSignal('');
  const [statusFilter, setStatusFilter] = createSignal<StatusFilter>('');

  // Drawer state
  const [openItem, setOpenItem] = createSignal<FeedbackItem | null>(null);
  const [draft, setDraft] = createSignal<{ priority: string; status: string; resolution: string }>({
    priority: 'normal',
    status: 'open',
    resolution: '',
  });
  const [saving, setSaving] = createSignal(false);

  const totalPages = createMemo(() => Math.max(1, Math.ceil(total() / perPage())));

  async function load(targetPage: number) {
    setLoading(true);
    setErr('');
    try {
      const params: { page: number; perPage: number; status?: string } = {
        page: targetPage,
        perPage: perPage(),
      };
      if (statusFilter()) params.status = statusFilter();
      const resp = await adminApi.listFeedback(params);
      setItems(resp.data);
      setTotal(resp.total);
      setPage(resp.page);
    } catch (e) {
      setErr(e instanceof Error ? e.message : '加载失败');
    } finally {
      setLoading(false);
    }
  }

  onMount(() => load(1));

  function applyFilter(s: StatusFilter) {
    setStatusFilter(s);
    load(1);
  }

  function openDrawer(it: FeedbackItem) {
    setOpenItem(it);
    setDraft({
      priority: it.priority || 'normal',
      status: it.status || 'open',
      resolution: it.resolution || '',
    });
  }

  async function saveDraft() {
    const it = openItem();
    if (!it) return;
    setSaving(true);
    try {
      const updated = await adminApi.updateFeedback(it.id, {
        priority: draft().priority,
        status: draft().status,
        resolution: draft().resolution || null,
      });
      // 局部刷新对应 row
      setItems((prev) => prev.map((x) => (x.id === it.id ? updated : x)));
      uiStore.toast.success('反馈已更新');
      setOpenItem(null);
    } catch (e) {
      uiStore.toast.error('保存失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setSaving(false);
    }
  }

  // 按 status 分组统计(基于当前 page,仅作 HeroCard 提示;严格统计需后端 aggregate 端点)
  const groupCounts = createMemo(() => {
    const counts: Record<string, number> = { open: 0, in_progress: 0, resolved: 0, closed: 0 };
    for (const it of items()) {
      const k = it.status || 'open';
      counts[k] = (counts[k] ?? 0) + 1;
    }
    return counts;
  });

  const columns = [
    {
      key: 'status',
      title: '状态',
      class: 'w-24',
      render: (r: FeedbackItem) => (
        <Badge variant={STATUS_VARIANTS[r.status] ?? 'default'} size="sm" dot>
          {STATUS_LABEL[r.status] ?? r.status}
        </Badge>
      ),
    },
    {
      key: 'priority',
      title: '优先级',
      class: 'w-20',
      render: (r: FeedbackItem) => (
        <Badge variant={PRIORITY_VARIANT[r.priority] ?? 'default'} size="sm">
          {PRIORITY_LABEL[r.priority] ?? r.priority}
        </Badge>
      ),
    },
    {
      key: 'category',
      title: '分类',
      class: 'w-24',
      render: (r: FeedbackItem) => (
        <Show when={r.category} fallback={<span class="text-content-tertiary">—</span>}>
          <span class="font-mono text-[12px]">{r.category}</span>
        </Show>
      ),
    },
    {
      key: 'body',
      title: '内容',
      render: (r: FeedbackItem) => (
        <p class="text-[13px] text-content truncate max-w-md" title={r.body}>{r.body}</p>
      ),
    },
    {
      key: 'route',
      title: '页面',
      render: (r: FeedbackItem) => (
        <Show when={r.route} fallback={<span class="text-content-tertiary">—</span>}>
          <span class="font-mono text-[11.5px] text-content-tertiary truncate inline-block max-w-[16ch]" title={r.route!}>
            {r.route}
          </span>
        </Show>
      ),
    },
    {
      key: 'createdAt',
      title: '时间',
      class: 'whitespace-nowrap',
      render: (r: FeedbackItem) => (
        <time class="text-[11.5px] tabular-nums text-content-tertiary">
          {new Date(r.createdAt).toLocaleString('zh-CN', { hour12: false })}
        </time>
      ),
    },
  ];

  return (
    <div class="space-y-4">
      <HeroCard
        eyebrow="工单流转"
        eyebrowVariant="warning"
        title="用户反馈"
        desc="用户通过 /api/feedback 提交的工单,支持 4 态流转(待处理 / 处理中 / 已解决 / 已关闭)、优先级、resolution 备注。点击行查看详情。"
        meta={[
          { value: total(), label: '总反馈数' },
          { value: groupCounts().open, label: '本页待处理' },
          { value: groupCounts().in_progress, label: '本页处理中' },
          { value: groupCounts().resolved, label: '本页已解决' },
        ]}
      />

      {/* status chip filter */}
      <div class="flex flex-wrap gap-2">
        <For each={[
          { v: '' as const, label: '全部' },
          { v: 'open' as const, label: STATUS_LABEL.open },
          { v: 'in_progress' as const, label: STATUS_LABEL.in_progress },
          { v: 'resolved' as const, label: STATUS_LABEL.resolved },
          { v: 'closed' as const, label: STATUS_LABEL.closed },
        ]}>
          {(f) => (
            <button
              type="button"
              onClick={() => applyFilter(f.v)}
              aria-pressed={statusFilter() === f.v}
              class={`px-3 py-1 rounded-pill text-[12px] font-medium transition-colors duration-fast ease-out-expo focus-ring-soft ${
                statusFilter() === f.v
                  ? 'bg-accent text-white shadow-elevation-1'
                  : 'bg-surface-secondary text-content-secondary hover:bg-surface-tertiary'
              }`}
            >
              {f.label}
            </button>
          )}
        </For>
      </div>

      <Card padding="none">
        <Show when={!loading()} fallback={<div class="py-12 flex justify-center"><Spinner size="lg" /></div>}>
          <Show
            when={!err()}
            fallback={
              <div class="py-12 px-4 flex flex-col items-center gap-3 text-center">
                <p class="text-error text-sm">{err()}</p>
                <Button variant="ghost" size="sm" onClick={() => load(page())}>重试</Button>
              </div>
            }
          >
            <Show when={items().length > 0} fallback={<Empty title="暂无反馈" description={statusFilter() ? '此 status 下没有匹配记录' : '还没有用户提交过反馈'} />}>
              <Table
                columns={columns}
                data={items()}
                onRowClick={(r) => openDrawer(r)}
                aria-label="用户反馈列表"
                caption="点击行打开详情抽屉编辑状态 / 优先级 / 解决备注"
              />
            </Show>
          </Show>
        </Show>

        <Show when={!loading() && totalPages() > 1}>
          <div class="flex items-center justify-between gap-2 p-3 border-t border-border-hairline">
            <span class="text-caption">第 {page()} / {totalPages()} 页</span>
            <Pagination page={page()} total={total()} pageSize={perPage()} onChange={load} />
          </div>
        </Show>
      </Card>

      {/* 详情抽屉 */}
      <Drawer
        open={!!openItem()}
        onClose={() => setOpenItem(null)}
        title="反馈详情"
        width={400}
        headerActions={
          <Show when={openItem()}>
            <Badge variant={STATUS_VARIANTS[openItem()!.status] ?? 'default'} size="sm" dot>
              {STATUS_LABEL[openItem()!.status] ?? openItem()!.status}
            </Badge>
          </Show>
        }
        footer={
          <div class="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setOpenItem(null)}>取消</Button>
            <Button size="sm" loading={saving()} onClick={saveDraft}>保存</Button>
          </div>
        }
      >
        <Show when={openItem()}>
          {(it) => (
            <div class="space-y-4 text-[13px]">
              {/* 元信息 */}
              <dl class="grid grid-cols-[80px_1fr] gap-y-2 gap-x-3 text-content-secondary">
                <dt>反馈 ID</dt>
                <dd class="font-mono text-[11.5px] text-content break-all">{it().id}</dd>
                <dt>用户 ID</dt>
                <dd class="font-mono text-[11.5px] text-content break-all">{it().userId}</dd>
                <dt>提交时间</dt>
                <dd class="font-mono tabular-nums text-[11.5px] text-content">
                  {new Date(it().createdAt).toLocaleString('zh-CN', { hour12: false })}
                </dd>
                <Show when={it().route}>
                  <dt>来源页面</dt>
                  <dd class="font-mono text-[11.5px] text-content break-all">{it().route}</dd>
                </Show>
                <Show when={it().category}>
                  <dt>分类</dt>
                  <dd class="text-content">{it().category}</dd>
                </Show>
                <Show when={it().assigneeAdminId}>
                  <dt>处理人</dt>
                  <dd class="font-mono text-[11.5px] text-content">{it().assigneeAdminId}</dd>
                </Show>
                <Show when={it().resolvedAt}>
                  <dt>解决时间</dt>
                  <dd class="font-mono tabular-nums text-[11.5px] text-content">
                    {new Date(it().resolvedAt!).toLocaleString('zh-CN', { hour12: false })}
                  </dd>
                </Show>
              </dl>

              {/* 反馈正文 */}
              <div>
                <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1.5">反馈内容</p>
                <div class="rounded-md bg-surface-secondary px-3 py-2 text-content whitespace-pre-wrap break-words leading-relaxed">
                  {it().body}
                </div>
              </div>

              {/* m022:设备指纹快照 + 答题上下文快照(老客户端无,折叠默认收起) */}
              <Show when={it().deviceProfile}>
                <Collapsible title="设备指纹快照" badge="m022">
                  <pre class="text-[11px] font-mono text-content-secondary bg-surface-secondary rounded-md p-2 overflow-x-auto max-h-64">
                    {JSON.stringify(it().deviceProfile, null, 2)}
                  </pre>
                </Collapsible>
              </Show>
              <Show when={it().answerSnapshot}>
                <Collapsible title="答题上下文快照" badge="m022">
                  <pre class="text-[11px] font-mono text-content-secondary bg-surface-secondary rounded-md p-2 overflow-x-auto max-h-64">
                    {JSON.stringify(it().answerSnapshot, null, 2)}
                  </pre>
                </Collapsible>
              </Show>

              {/* 状态 / 优先级 编辑 */}
              <div class="grid grid-cols-2 gap-3">
                <div>
                  <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">状态</p>
                  <Select
                    value={draft().status}
                    onChange={(e) => setDraft({ ...draft(), status: e.currentTarget.value })}
                    options={[
                      { value: 'open', label: STATUS_LABEL.open },
                      { value: 'in_progress', label: STATUS_LABEL.in_progress },
                      { value: 'resolved', label: STATUS_LABEL.resolved },
                      { value: 'closed', label: STATUS_LABEL.closed },
                    ]}
                  />
                </div>
                <div>
                  <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">优先级</p>
                  <Select
                    value={draft().priority}
                    onChange={(e) => setDraft({ ...draft(), priority: e.currentTarget.value })}
                    options={[
                      { value: 'urgent', label: PRIORITY_LABEL.urgent },
                      { value: 'high', label: PRIORITY_LABEL.high },
                      { value: 'normal', label: PRIORITY_LABEL.normal },
                      { value: 'low', label: PRIORITY_LABEL.low },
                    ]}
                  />
                </div>
              </div>

              {/* 解决备注 */}
              <div>
                <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">解决备注</p>
                <textarea
                  value={draft().resolution}
                  onInput={(e) => setDraft({ ...draft(), resolution: e.currentTarget.value })}
                  rows={3}
                  placeholder="resolved / closed 时填写处理说明,供后续审计"
                  class="w-full px-3 py-2 rounded-md border border-border bg-surface text-[13px] focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/30 transition"
                />
              </div>
            </div>
          )}
        </Show>
      </Drawer>
    </div>
  );
}
