import { type JSX, createSignal, createResource, createMemo, Show, For } from 'solid-js';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Modal } from '@/components/ui/Modal';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import type { BroadcastHistoryEntry } from '@/types/admin';
import './broadcast.css';

/**
 * 系统广播页（对齐设计稿 broadcast.html）。
 *
 * 单一撰写表单 + 历史表 + 右栏。后端契约：
 *   - GET  /api/admin/broadcast          → { stats, broadcasts }（近 30 天看板）
 *   - POST /api/admin/broadcast          → { sent, broadcastId }（全员广播，写入历史）
 *   - POST /api/admin/broadcast/preview  → { matched, total }（取全员总数做批次预估）
 */

const BATCH_SIZE = 100;
const TITLE_MAX = 200;
const MSG_MAX = 10000;
const PAGE_SIZE = 20;

type HistoryFilter = 'all' | 'week' | 'failed';

interface Template {
  cls: 't-warn' | 't-update' | 't-info' | 't-success';
  icon: () => JSX.Element;
  name: string;
  desc: string;
  title: string;
  message: string;
}

const TEMPLATES: Template[] = [
  {
    cls: 't-warn',
    name: '计划内维护通知',
    desc: '含影响范围、持续时间、应对建议',
    title: '计划内维护 — [日期 时段]',
    message:
      '我们将在 [日期 时段] 进行计划内系统维护，预计持续 [N] 分钟。\n\n影响范围：\n· AMAS 推荐与决策记录暂停\n· LLM 顾问与广播通知暂停\n· 用户答题数据本地保留，恢复后自动同步\n\n给您带来的不便我们深表歉意。',
    icon: () => (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><line x1="12" y1="8" x2="12" y2="12" /><line x1="12" y1="16" x2="12.01" y2="16" /></svg>
    ),
  },
  {
    cls: 't-update',
    name: '版本通告',
    desc: '新版本发布 + 主要变化',
    title: 'vX.Y.Z 现已可用 — [一句话亮点]',
    message:
      '新版本 vX.Y.Z 已发布。\n\n主要变化：\n· [feat] [...]\n· [fix] [...]\n· [perf] [...]\n\n客户端会在下次启动时自动检测，也可在「设置 → 关于」手动触发更新。',
    icon: () => (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" /></svg>
    ),
  },
  {
    cls: 't-info',
    name: '周报 / 月报',
    desc: '运营节奏类公告',
    title: '本周更新摘要 — [主题]',
    message:
      '本周更新摘要：\n\n1. [...] \n2. [...] \n3. [...] \n\n详细 CHANGELOG 可在「设置 → 更新」查看。',
    icon: () => (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><polyline points="12 6 12 12 16 14" /></svg>
    ),
  },
  {
    cls: 't-success',
    name: '安全公告',
    desc: 'CVE / 漏洞修复 / 强升级',
    title: '安全更新 vX.Y.Z — 强烈建议立即升级',
    message:
      '为修复 [简述] 问题，我们发布了安全更新 vX.Y.Z。\n\n请尽快升级。客户端可在「设置 → 关于 → 检查更新」中触发。\n\n如有问题请通过反馈中心联系我们。',
    icon: () => (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /></svg>
    ),
  },
];

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`;
}

/** 设计稿格式：MM-DD HH:mm */
function fmtTs(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return `${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

function fmtRelative(iso: string): string {
  const d = new Date(iso).getTime();
  if (Number.isNaN(d)) return '';
  const diff = Date.now() - d;
  if (diff < 0) return '刚刚';
  const min = Math.floor(diff / 60000);
  if (min < 1) return '刚刚';
  if (min < 60) return `${min} min 前`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} 小时前`;
  const day = Math.floor(hr / 24);
  return `${day} 天前`;
}

function avatarText(author: string): string {
  const a = (author || '').trim();
  if (!a) return '?';
  // ASCII 取前两位大写首字母，否则取末字（中文姓名取后两字更自然，这里取首字）
  if (/^[\x00-\x7F]+$/.test(a)) return a.slice(0, 2).toUpperCase();
  return a.slice(0, 1);
}

export default function BroadcastPage() {
  const [title, setTitle] = createSignal('');
  const [message, setMessage] = createSignal('');
  const [sending, setSending] = createSignal(false);
  const [showConfirm, setShowConfirm] = createSignal(false);
  const [showConfirmClear, setShowConfirmClear] = createSignal(false);
  const [filter, setFilter] = createSignal<HistoryFilter>('all');
  const [detail, setDetail] = createSignal<BroadcastHistoryEntry | null>(null);
  // 进度提示：null=隐藏；in-flight=请求中（不定进度）；done=填满后短暂保留
  const [progress, setProgress] = createSignal<{ phase: 'in-flight' | 'done'; sent: number } | null>(null);
  // 历史分页：offset 驱动，total 取自后端 pagination，不再静默截断 50 条
  const [offset, setOffset] = createSignal(0);

  // 看板数据：近 30 天统计 + 历史（按 offset/limit/filter 翻页）。
  // E2:filter 下推后端跨页生效，资源 source 含 offset+filter，二者任一变即重取。
  const [board, { refetch: refetchBoard }] = createResource(
    () => ({ off: offset(), f: filter() }),
    ({ off, f }) => adminApi.listBroadcasts({ offset: off, limit: PAGE_SIZE, filter: f }),
  );
  // 全员总数（批次预估）
  const [preview, { refetch: refetchPreview }] = createResource(() => adminApi.broadcastPreview({}));

  const total = createMemo(() => preview()?.total ?? 0);
  const batches = createMemo(() => Math.max(1, Math.ceil(total() / BATCH_SIZE)));

  const stats = createMemo(() => board()?.stats ?? { total: 0, totalSent: 0, avgReadRate: 0, online: 0 });
  const broadcasts = createMemo<BroadcastHistoryEntry[]>(() => board()?.broadcasts ?? []);

  // E2:分页 total 来自后端 pagination，已是「按 filter 过滤后的计数」，故页脚跨页正确。
  const totalHistory = createMemo(() => board()?.pagination?.total ?? broadcasts().length);
  const pageStart = createMemo(() => (totalHistory() === 0 ? 0 : offset() + 1));
  const pageEnd = createMemo(() => offset() + broadcasts().length);
  const hasPrev = createMemo(() => offset() > 0);
  const hasNext = createMemo(() => offset() + broadcasts().length < totalHistory());

  function prevPage() {
    if (hasPrev()) setOffset(Math.max(0, offset() - PAGE_SIZE));
  }
  function nextPage() {
    if (hasNext()) setOffset(offset() + PAGE_SIZE);
  }

  // E2:切换筛选后端跨页重取，需回到第一页（否则当前 offset 可能越过新结果集）。
  function changeFilter(f: HistoryFilter) {
    if (filter() === f) return;
    setOffset(0);
    setFilter(f);
  }

  const titleLen = createMemo(() => title().length);
  const msgLen = createMemo(() => message().length);
  const canSend = createMemo(() => !!title().trim() && !!message().trim() && !sending());

  function pickTemplate(t: Template) {
    setTitle(t.title);
    setMessage(t.message);
  }

  function onClear() {
    if (!title() && !message()) return;
    setShowConfirmClear(true);
  }
  function doClear() {
    setShowConfirmClear(false);
    setTitle('');
    setMessage('');
  }

  function onSendClick() {
    if (!title().trim() || !message().trim()) {
      uiStore.toast.warning('请填写标题和正文');
      return;
    }
    setShowConfirm(true);
  }

  async function doSend() {
    setShowConfirm(false);
    setSending(true);
    setProgress({ phase: 'in-flight', sent: 0 });
    try {
      const res = await adminApi.broadcast({ title: title(), message: message() });
      const sent = res.sent ?? 0; // BroadcastPage 不排程,sent 必有;?? 0 仅满足类型
      setProgress({ phase: 'done', sent });
      uiStore.toast.success(`广播已发送 · 写入 ${sent.toLocaleString()} 条通知`);
      setTitle('');
      setMessage('');
      // 刷新历史与统计
      await Promise.all([refetchBoard(), refetchPreview()]);
      // 进度提示填满后短暂保留再隐藏
      setTimeout(() => setProgress(null), 1400);
    } catch (err: unknown) {
      setProgress(null);
      uiStore.toast.error('发送失败', err instanceof Error ? err.message : '');
    } finally {
      setSending(false);
    }
  }

  function exportCsv() {
    const list = broadcasts();
    if (list.length === 0) {
      uiStore.toast.warning('暂无可导出的历史');
      return;
    }
    const esc = (v: string) => `"${String(v).replace(/"/g, '""')}"`;
    const head = 'id,createdAt,author,title,message,sentCount,readCount,readRate';
    const rows = list.map((b) =>
      [b.id, b.createdAt, b.author, b.title, b.message, b.sentCount, b.readCount, b.readRate]
        .map((v) => esc(String(v)))
        .join(','),
    );
    const blob = new Blob([`${head}\n${rows.join('\n')}`], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `broadcast-history-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }

  function onRefresh() {
    refetchBoard();
    refetchPreview();
  }

  return (
    <div class="broadcast">
      <div class="page-header">
        <div class="row spread">
          <div>
            <h1 class="page-title">系统广播</h1>
            <p class="page-desc">
              向所有用户的通知中心写入一条系统消息。后端分批 100 条入库以避免内存峰值，幂等 ID 防重，写入完成后即对在线用户立即可见。
            </p>
          </div>
          <div class="head-actions">
            <button type="button" class="btn btn-secondary" onClick={exportCsv}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" /></svg>
              导出历史
            </button>
            <button type="button" class="btn btn-secondary" onClick={onRefresh}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" /></svg>
              刷新
            </button>
          </div>
        </div>
      </div>

      <div class="bc-grid">
        {/* ====== LEFT: compose + history ====== */}
        <div>
          <div class="bc-compose animate-fade-in-up">
            <div class="bc-compose-head">
              <div class="ic">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 11l18-5v12L3 14v-3z" /><path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" /></svg>
              </div>
              <h3>撰写广播</h3>
              <span class="sub">POST /api/admin/broadcast</span>
            </div>
            <div class="bc-form">
              <div class="bc-field">
                <div class="bc-field-label">
                  <label for="bc-title">标题 <span class="bc-req">*</span></label>
                  <span
                    class="count"
                    classList={{ 'is-warn': titleLen() > 150 && titleLen() < TITLE_MAX, 'is-bad': titleLen() >= TITLE_MAX }}
                  >
                    {titleLen().toLocaleString()} / {TITLE_MAX.toLocaleString()}
                  </span>
                </div>
                <input
                  id="bc-title"
                  class="bc-input"
                  type="text"
                  maxlength={TITLE_MAX}
                  disabled={sending()}
                  value={title()}
                  onInput={(e) => setTitle(e.currentTarget.value)}
                  placeholder="例：12 月 5 日 02:00–04:00 计划内维护"
                />
              </div>

              <div class="bc-field">
                <div class="bc-field-label">
                  <label for="bc-message">正文 <span class="bc-req">*</span></label>
                  <span
                    class="count"
                    classList={{ 'is-warn': msgLen() > 8000 && msgLen() < MSG_MAX, 'is-bad': msgLen() >= MSG_MAX }}
                  >
                    {msgLen().toLocaleString()} / {MSG_MAX.toLocaleString()}
                  </span>
                </div>
                <textarea
                  id="bc-message"
                  class="bc-textarea"
                  maxlength={MSG_MAX}
                  disabled={sending()}
                  value={message()}
                  onInput={(e) => setMessage(e.currentTarget.value)}
                  placeholder={'支持纯文本与 \\n 换行。客户端通知中心会原样渲染。\n\n建议：\n1. 第一行用主动语态说明影响范围与持续时间\n2. 第二段写应对措施 / 用户需要做的事情\n3. 末尾留备用联系方式（可选）'}
                />
              </div>

              <div class="bc-field">
                <div class="bc-field-label">
                  <label>实时预览（客户端通知中心样式）</label>
                  <span class="count">{msgLen().toLocaleString()} 字</span>
                </div>
                <div class="bc-preview">
                  <div class="pv-head">
                    <div class="pv-ic">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 11l18-5v12L3 14v-3z" /><path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" /></svg>
                    </div>
                    <div class="pv-from">WordForge 系统</div>
                    <div class="pv-time">刚刚</div>
                  </div>
                  <div class="pv-title" classList={{ 'pv-empty': !title() }}>
                    {title() || '（标题预览）'}
                  </div>
                  <div class="pv-msg" classList={{ 'pv-empty': !message() }}>
                    {message() || '（正文预览。撰写时会同步刷新。）'}
                  </div>
                </div>
              </div>
            </div>
            <div class="bc-actions">
              <span class="meta">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 3" /></svg>
                预计写入 <strong>{total().toLocaleString()}</strong> 条通知 · 分 <strong>{batches().toLocaleString()}</strong> 批
              </span>
              <span class="spacer" />
              <button type="button" class="btn btn-ghost" disabled={sending()} onClick={onClear}>清空</button>
              <button type="button" class="btn btn-primary" disabled={!canSend()} onClick={onSendClick}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="22" y1="2" x2="11" y2="13" /><polygon points="22 2 15 22 11 13 2 9 22 2" /></svg>
                发送广播
              </button>
            </div>
          </div>

          {/* history */}
          <div class="bc-history animate-fade-in-up bc-d80">
            <div class="bc-history-head">
              <h3>最近广播</h3>
              <span class="sub">近 30 天 · 后端无定时 / 无撤回</span>
              <div class="filter">
                <button type="button" classList={{ 'is-active': filter() === 'all' }} onClick={() => changeFilter('all')}>全部</button>
                <button type="button" classList={{ 'is-active': filter() === 'week' }} onClick={() => changeFilter('week')}>本周</button>
                <button type="button" classList={{ 'is-active': filter() === 'failed' }} onClick={() => changeFilter('failed')}>失败</button>
              </div>
            </div>
            <div>
              <Show
                when={!board.loading}
                fallback={<div class="bc-empty">加载中…</div>}
              >
                <Show when={!board.error} fallback={<div class="bc-empty">加载失败，请点右上角刷新重试</div>}>
                  <Show when={broadcasts().length > 0} fallback={<div class="bc-empty">暂无广播记录</div>}>
                    <For each={broadcasts()}>
                      {(b) => {
                        const pct = Math.round((b.readRate || 0) * 100);
                        return (
                          <div class="bc-row">
                            <div class="ts">
                              {fmtTs(b.createdAt)} <span>{fmtRelative(b.createdAt)}</span>
                            </div>
                            <div class="ti">
                              {b.title}
                              <div class="desc">{b.message}</div>
                            </div>
                            <div class="au">
                              <div class="avatar">{avatarText(b.author)}</div>
                              {b.author}
                            </div>
                            <div class="num">
                              {b.sentCount.toLocaleString()} <span>送达</span>
                            </div>
                            <div class="read">
                              <div class="pct">{pct}%</div>
                              <div class="bar"><span style={{ width: `${pct}%` }} /></div>
                            </div>
                            <div class="ops">
                              <button type="button" onClick={() => setDetail(b)}>详情</button>
                            </div>
                          </div>
                        );
                      }}
                    </For>
                  </Show>
                </Show>
              </Show>
            </div>
            <Show when={!board.loading && !board.error && totalHistory() > 0}>
              <div class="bc-pager">
                <span class="bc-pager-info">
                  第 <strong>{pageStart().toLocaleString()}</strong>–<strong>{pageEnd().toLocaleString()}</strong> 条 · 共 <strong>{totalHistory().toLocaleString()}</strong> 条
                </span>
                <span class="spacer" />
                <button type="button" class="bc-pager-btn" disabled={!hasPrev()} onClick={prevPage}>
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 18 9 12 15 6" /></svg>
                  上一页
                </button>
                <button type="button" class="bc-pager-btn" disabled={!hasNext()} onClick={nextPage}>
                  下一页
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6" /></svg>
                </button>
              </div>
            </Show>
          </div>
        </div>

        {/* ====== RIGHT: stats + templates + constraints ====== */}
        <div>
          <div class="bc-side-stats animate-fade-in-up bc-d40">
            <h4>近 30 天</h4>
            <div class="bc-stat-grid">
              <div class="bc-stat is-up">
                <div class="v">{stats().total.toLocaleString()}</div>
                <div class="l">广播总数</div>
              </div>
              <div class="bc-stat">
                <div class="v">{stats().totalSent.toLocaleString()}</div>
                <div class="l">总送达</div>
              </div>
              <div class="bc-stat">
                <div class="v">{Math.round(stats().avgReadRate * 100)}<small>%</small></div>
                <div class="l">平均阅读率</div>
              </div>
              <div class="bc-stat is-up">
                <div class="v">{stats().online.toLocaleString()}</div>
                <div class="l">当前在线</div>
              </div>
            </div>
          </div>

          <div class="bc-templates animate-fade-in-up bc-d80">
            <h4>快捷模板</h4>
            <For each={TEMPLATES}>
              {(t) => (
                <button type="button" class={`bc-tpl ${t.cls}`} onClick={() => pickTemplate(t)}>
                  <div class="ic">{t.icon()}</div>
                  <div class="tx">
                    <strong>{t.name}</strong>
                    <div class="d">{t.desc}</div>
                  </div>
                </button>
              )}
            </For>
          </div>

          <div class="bc-constraints">
            <h5>后端契约</h5>
            <ul>
              <li>调用 <code>POST /api/admin/broadcast</code>，body 为 <code>{'{ title, message }'}</code></li>
              <li>title 1–200 字符，message 1–10,000 字符；超长服务端返回 <code>INVALID_*</code></li>
              <li>分批 100 条入库，避免一次性加载所有用户导致内存峰值</li>
              <li>每条广播对应一个 UUID <code>broadcastId</code>，幂等防重，单次最多 ~3K 通知耗时 1–2 秒</li>
              <li>无定时发送、无撤回 — 一旦点击发送即不可逆，请谨慎</li>
              <li>SSE 实时通道仅用于更新通告 <code>broadcast_update</code>，普通消息走通知中心</li>
            </ul>
          </div>
        </div>
      </div>

      {/* 发送确认弹窗（复用 ConfirmDialog） */}
      <ConfirmDialog
        open={showConfirm()}
        title="确认发送广播？"
        variant="warning"
        confirmText="立即发送"
        loading={sending()}
        message={
          <>
            <p>
              广播一旦发出 <strong class="text-content">不可撤回</strong>，会立即写入所有用户的通知中心，并通过 SSE 推送给在线客户端。请最后确认下面的内容。
            </p>
            <div class="mt-3 flex flex-col gap-2">
              <div class="flex items-center justify-between px-2.5 py-2 rounded bg-surface-secondary">
                <span class="text-[11.5px] text-content-tertiary">收件人</span>
                <span class="font-mono text-xs text-content tabular-nums">{total().toLocaleString()} 用户 ({batches().toLocaleString()} 批)</span>
              </div>
              <div class="flex items-center justify-between px-2.5 py-2 rounded bg-surface-secondary">
                <span class="text-[11.5px] text-content-tertiary">标题长度</span>
                <span class="font-mono text-xs text-content tabular-nums">{titleLen()} 字符</span>
              </div>
              <div class="flex items-center justify-between px-2.5 py-2 rounded bg-surface-secondary">
                <span class="text-[11.5px] text-content-tertiary">正文长度</span>
                <span class="font-mono text-xs text-content tabular-nums">{msgLen()} 字符</span>
              </div>
              <div class="flex items-center justify-between px-2.5 py-2 rounded bg-surface-secondary">
                <span class="text-[11.5px] text-content-tertiary">幂等 ID</span>
                <span class="font-mono text-[10.5px] text-content">将在发送时生成</span>
              </div>
            </div>
          </>
        }
        onConfirm={doSend}
        onCancel={() => setShowConfirm(false)}
      />

      {/* 清空二次确认 */}
      <ConfirmDialog
        open={showConfirmClear()}
        title="清空已撰写的内容？"
        variant="warning"
        confirmText="清空"
        message={<p>标题与正文将被清空，此操作不可恢复。</p>}
        onConfirm={doClear}
        onCancel={() => setShowConfirmClear(false)}
      />

      {/* 历史详情（只读） */}
      <Modal open={!!detail()} onClose={() => setDetail(null)} title="广播详情" size="md">
        <Show when={detail()}>
          {(d) => (
            <div class="space-y-3 text-sm">
              <div>
                <div class="text-[11.5px] text-content-tertiary mb-1">标题</div>
                <div class="font-medium text-content">{d().title}</div>
              </div>
              <div>
                <div class="text-[11.5px] text-content-tertiary mb-1">正文</div>
                <div class="text-content-secondary whitespace-pre-wrap break-words">{d().message}</div>
              </div>
              <div class="grid grid-cols-2 gap-3 pt-1">
                <div>
                  <div class="text-[11.5px] text-content-tertiary mb-1">作者</div>
                  <div class="text-content">{d().author}</div>
                </div>
                <div>
                  <div class="text-[11.5px] text-content-tertiary mb-1">发送时间</div>
                  <div class="font-mono text-content tabular-nums">{fmtTs(d().createdAt)}</div>
                </div>
                <div>
                  <div class="text-[11.5px] text-content-tertiary mb-1">送达</div>
                  <div class="font-mono text-content tabular-nums">{d().sentCount.toLocaleString()}</div>
                </div>
                <div>
                  <div class="text-[11.5px] text-content-tertiary mb-1">阅读率</div>
                  <div class="font-mono text-content tabular-nums">{Math.round((d().readRate || 0) * 100)}%（{d().readCount.toLocaleString()} 已读）</div>
                </div>
              </div>
            </div>
          )}
        </Show>
      </Modal>

      {/* 发送进度提示（真实 in-flight 状态驱动，非伪造逐批 SSE） */}
      <Show when={progress()}>
        {(p) => (
          <div class="bc-progress-toast">
            <div class="ph">
              <div class="ic">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" /></svg>
              </div>
              <strong>{p().phase === 'done' ? '广播已写入通知中心' : '正在分批写入通知…'}</strong>
            </div>
            <div class="bar">
              <span classList={{ 'is-indeterminate': p().phase === 'in-flight' }} style={p().phase === 'done' ? { width: '100%' } : undefined} />
            </div>
            <div class="stats">
              <Show
                when={p().phase === 'done'}
                fallback={<span>提交中 · 全员 {total().toLocaleString()} 用户 / {batches().toLocaleString()} 批</span>}
              >
                <span>已写入 <strong>{p().sent.toLocaleString()}</strong> 条通知</span>
              </Show>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
