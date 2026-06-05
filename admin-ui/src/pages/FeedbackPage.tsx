import { createSignal, createResource, createMemo, createEffect, For, Show, batch } from 'solid-js';
import { PageHeader } from '@/components/ui/PageHeader';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Switch } from '@/components/ui/Switch';
import { Sparkline } from '@/components/ui/Sparkline';
import { adminApi } from '@/api/admin';
import { ApiError } from '@/api/http';
import { uiStore } from '@/stores/ui';
import { formatRelativeTime } from '@/utils/formatters';
import type { FeedbackItem, FeedbackStats, FeedbackDetail, FeedbackEvent } from '@/types/admin';
import { AnnouncementManager } from './feedback/AnnouncementManager';
import './feedback.css';

/**
 * 反馈中心 —— m030 工单化收件箱。
 * 布局对齐设计稿 admin后端/feedback.html:page-header / KPI 条 / 分类 tab /
 * 筛选 btn-group / fb-split(左列表右详情) / context / content / timeline / composer。
 */

// SLA 计时常量(分钟)
const SLA_MINUTES = 30;

// 后端 category 值 → CSS class 后缀(投诉是 complain 非 complaint)
const CAT_CLASS: Record<string, string> = {
  bug: 'bug',
  feature: 'feature',
  complaint: 'complain',
  complain: 'complain',
  praise: 'praise',
  support: 'support',
};
const CAT_LABEL: Record<string, string> = {
  bug: 'Bug',
  feature: '建议',
  complaint: '投诉',
  complain: '投诉',
  praise: '表扬',
  support: '咨询',
};

// 后端 priority 值 → 设计图 P0-P3 档位
const PRI_CLASS: Record<string, string> = {
  urgent: 'P0',
  high: 'P1',
  normal: 'P2',
  low: 'P3',
};

// 分类 tab 定义(计数取自 stats.byCategory)
const CAT_TABS: { value: string; label: string; icBg: string; icFg: string; icon: string }[] = [
  { value: '', label: '全部', icBg: 'var(--surface-tertiary)', icFg: 'var(--content)', icon: 'M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z' },
  { value: 'bug', label: 'Bug', icBg: 'var(--error-light)', icFg: 'var(--error-strong)', icon: 'M12 2a10 10 0 100 20 10 10 0 000-20zM12 8v4M12 16h.01' },
  { value: 'feature', label: '功能建议', icBg: 'var(--info-light)', icFg: 'var(--info-strong)', icon: 'M13 2 3 14h9l-1 8 10-12h-9l1-8z' },
  { value: 'complaint', label: '投诉', icBg: 'var(--warning-light)', icFg: 'var(--warning-strong)', icon: 'M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z' },
  { value: 'praise', label: '表扬', icBg: 'var(--success-light)', icFg: 'var(--success-strong)', icon: 'M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3z' },
];

// 头像渐变色(按分类区分)
const AVATAR_GRAD: Record<string, string> = {
  bug: 'linear-gradient(135deg, var(--error), oklch(54% 0.22 28))',
  feature: 'linear-gradient(135deg, var(--info), oklch(56% 0.18 230))',
  complaint: 'linear-gradient(135deg, var(--warning), oklch(58% 0.18 60))',
  complain: 'linear-gradient(135deg, var(--warning), oklch(58% 0.18 60))',
  praise: 'linear-gradient(135deg, var(--success), oklch(56% 0.14 162))',
  support: 'linear-gradient(135deg, var(--accent), oklch(56% 0.22 290))',
};

// 回复快捷文案
const SNIPPETS = [
  '已修复,将在下个版本上线。',
  '已转研发跟进。',
  '感谢反馈,能否补充更多信息(复现步骤 / 截图)?',
  '感谢你的支持与肯定!',
  '工单将在 30 天后自动关闭。',
];

// 筛选维度
type FilterKey = 'unread' | 'assigned' | 'resolved' | 'all';

function catClass(c: string | null): string {
  return CAT_CLASS[c ?? ''] ?? 'support';
}
function catLabel(c: string | null): string {
  return CAT_LABEL[c ?? ''] ?? (c ?? '其他');
}
function priClass(p: string): string {
  return PRI_CLASS[p] ?? 'P3';
}
function avatarGrad(c: string | null): string {
  return AVATAR_GRAD[c ?? ''] ?? 'linear-gradient(135deg, var(--accent), oklch(56% 0.22 290))';
}
// 头像首字:中文取首字,英文取首字母大写
function avatarChar(it: FeedbackItem): string {
  const n = (it.userName || it.userId || '?').trim();
  return n ? n[0].toUpperCase() : '?';
}
// body 首行作为主题
function subjectOf(body: string): string {
  return (body || '').split('\n')[0].trim() || '(无内容)';
}
// body 余下作为预览
function previewOf(body: string): string {
  const lines = (body || '').split('\n');
  return lines.slice(1).join(' ').trim() || lines[0]?.trim() || '';
}
// 是否像代码/栈(简单启发:含多行且有典型代码符号)
function looksLikeCode(body: string): boolean {
  return /\n/.test(body) && /[{};]|0x[0-9a-f]|Exception|Thread|at .+\(.+\)/.test(body);
}
// SLA 剩余分钟(从创建至今,30min 倒计时,最小 0)
function slaRemaining(createdAt: string): number {
  const elapsed = (Date.now() - new Date(createdAt).getTime()) / 60000;
  return Math.max(0, Math.round(SLA_MINUTES - elapsed));
}
// 事件 kind → 时间线样式 modifier
function eventClass(kind: string): string {
  if (kind === 'resolved') return 'is-resolved';
  if (kind === 'assigned') return 'is-assigned';
  if (kind === 'reply') return 'is-reply';
  return '';
}
// 周环比徽章:改善=绿(up class)、恶化=红(down class)、~0=灰(flat);箭头反映数值实际增减
function deltaBadge(
  delta: number | null,
  opts: { unit: string; goodWhenDown: boolean; round?: boolean; pct?: boolean; fixed?: number },
) {
  if (delta == null) return <span class="delta flat">—</span>;
  const eps = opts.pct ? 0.0005 : 0.05;
  if (Math.abs(delta) < eps) return <span class="delta flat">持平</span>;
  const isGood = opts.goodWhenDown ? delta < 0 : delta > 0;
  const arrow = delta < 0 ? '▼' : '▲';
  const mag = Math.abs(delta);
  const text = opts.pct
    ? `${(mag * 100).toFixed(1)}%`
    : opts.fixed != null
      ? mag.toFixed(opts.fixed)
      : opts.round
        ? String(Math.round(mag))
        : String(mag);
  const suffix = opts.pct ? '' : opts.unit;
  return <span class={`delta ${isGood ? 'up' : 'down'}`}>{arrow} {text}{suffix}</span>;
}
// 列表行右侧语义 chip:由 csat/priority/dedup 派生(无投票数据源,故不臆造"+N 投票")
function rowChip(it: FeedbackItem): { label: string; cls: string } | null {
  if (it.csatScore != null) return { label: `CSAT ${it.csatScore}★`, cls: 'chip-success' };
  if (it.priority === 'urgent' && it.category === 'bug') return { label: '崩溃', cls: 'chip-error' };
  if (it.priority === 'urgent') return { label: '紧急', cls: 'chip-error' };
  if (it.status === 'in_progress') return { label: '处理中', cls: 'chip-info' };
  if (it.category === 'complaint') return { label: '投诉', cls: 'chip-warning' };
  return null;
}

export default function FeedbackPage() {
  // 列表筛选状态
  const [category, setCategory] = createSignal('');
  const [filter, setFilter] = createSignal<FilterKey>('all');
  const [search, setSearch] = createSignal('');
  const [selectedId, setSelectedId] = createSignal<string | null>(null);

  // 公告 / FAQ 管理弹窗
  const [annOpen, setAnnOpen] = createSignal(false);

  // 高级筛选面板(展开态 + 各维度;platform/version/assignee/time 后端列表端点不支持,客户端过滤)
  const [advOpen, setAdvOpen] = createSignal(false);
  const [fPlatform, setFPlatform] = createSignal('');
  const [fVersion, setFVersion] = createSignal('');
  const [fAssignee, setFAssignee] = createSignal('');
  const [fSince, setFSince] = createSignal(''); // YYYY-MM-DD,只保留该日期(含)之后创建的

  // composer 状态
  const [replyBody, setReplyBody] = createSignal('');
  const [pushInapp, setPushInapp] = createSignal(true);
  const [ccEmail, setCcEmail] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [savingDraft, setSavingDraft] = createSignal(false);
  const [acting, setActing] = createSignal(false);
  let composerRef: HTMLTextAreaElement | undefined;

  // KPI 统计
  const [stats, { refetch: refetchStats }] = createResource<FeedbackStats>(() =>
    adminApi.getFeedbackStats(),
  );

  // 列表(随 category / filter 变化重取)
  const listKey = createMemo(() => ({ category: category(), filter: filter() }));
  const [listResp, { refetch: refetchList }] = createResource(listKey, async (k) => {
    const params: Parameters<typeof adminApi.listFeedback>[0] = {
      page: 1,
      perPage: 50,
    };
    if (k.category) params.category = k.category;
    if (k.filter === 'unread') params.unread = true;
    else if (k.filter === 'assigned') params.assigned = true;
    else if (k.filter === 'resolved') params.status = 'resolved';
    return adminApi.listFeedback(params);
  });

  // 详情(随 selectedId 变化重取)
  const [detail, { refetch: refetchDetail }] = createResource<FeedbackDetail | null, string | null>(
    selectedId,
    async (id) => (id ? adminApi.getFeedbackDetail(id) : null),
  );

  // 选中工单后拉草稿;有草稿则回填 composer(尚未发送的回复)
  createEffect(() => {
    const id = selectedId();
    if (!id) return;
    // 重置为该工单态,随后异步覆盖
    batch(() => {
      setReplyBody('');
      setPushInapp(true);
      setCcEmail(false);
    });
    adminApi
      .getFeedbackDraft(id)
      .then((r) => {
        if (selectedId() !== id || !r.draft) return; // 期间已切换则丢弃
        batch(() => {
          setReplyBody(r.draft!.body);
          setPushInapp(r.draft!.pushInapp);
          setCcEmail(r.draft!.ccEmail);
        });
      })
      .catch(() => {
        /* 无草稿或失败:维持空白,不打扰 */
      });
  });

  // 平台 / 版本 facet:从当前列表派生(供高级筛选下拉)
  const platformOptions = createMemo(() => {
    const set = new Set<string>();
    for (const it of listResp()?.data ?? []) if (it.platform) set.add(it.platform);
    return Array.from(set).sort();
  });
  const versionOptions = createMemo(() => {
    const set = new Set<string>();
    for (const it of listResp()?.data ?? []) if (it.appVersion) set.add(it.appVersion);
    return Array.from(set).sort();
  });
  const assigneeOptions = createMemo(() => {
    const set = new Set<string>();
    for (const it of listResp()?.data ?? []) if (it.assigneeAdminId) set.add(it.assigneeAdminId);
    return Array.from(set).sort();
  });

  // 高级筛选是否生效(用于按钮高亮 + 清除可见性)
  const advActive = createMemo(() => !!(fPlatform() || fVersion() || fAssignee() || fSince()));

  // 客户端按搜索词 + 高级筛选过滤列表行
  const rows = createMemo(() => {
    const q = search().trim().toLowerCase();
    const plat = fPlatform();
    const ver = fVersion();
    const asg = fAssignee();
    const sinceMs = fSince() ? new Date(fSince()).getTime() : null;
    return (listResp()?.data ?? []).filter((it) => {
      if (q && !(
        it.body.toLowerCase().includes(q) ||
        (it.userName ?? '').toLowerCase().includes(q) ||
        it.userId.toLowerCase().includes(q)
      )) return false;
      if (plat && it.platform !== plat) return false;
      if (ver && it.appVersion !== ver) return false;
      if (asg === '__none__' ? it.assigneeAdminId != null : asg && it.assigneeAdminId !== asg) return false;
      if (sinceMs != null && new Date(it.createdAt).getTime() < sinceMs) return false;
      return true;
    });
  });

  function clearAdvanced() {
    batch(() => {
      setFPlatform('');
      setFVersion('');
      setFAssignee('');
      setFSince('');
    });
  }

  // 分类 tab 计数
  function catCount(value: string): number {
    // stats 处于 error 态时读取 accessor 会重抛,先判 .error 再取值,避免整站崩
    const s = stats.error ? undefined : stats();
    if (!s) return 0;
    if (value === '') return s.total;
    // complaint / complain 两种 key 兜底
    if (value === 'complaint') return (s.byCategory.complaint ?? 0) + (s.byCategory.complain ?? 0);
    return s.byCategory[value] ?? 0;
  }

  // 选中行 → 触发详情请求(后端 GET detail 会置 readAt)
  function selectRow(it: FeedbackItem) {
    setSelectedId(it.id);
  }

  // ── 操作 ──
  async function onMarkAllRead() {
    try {
      const r = await adminApi.markAllFeedbackRead();
      uiStore.toast.success('已全部标记已读', `更新 ${r.updated} 条`);
      refetchStats();
      refetchList();
    } catch (e) {
      uiStore.toast.error('操作失败', e instanceof Error ? e.message : '未知错误');
    }
  }

  async function onExportCsv() {
    try {
      const url = await adminApi.feedbackCsvUrl();
      const a = document.createElement('a');
      a.href = url;
      a.download = 'feedback.csv';
      document.body.appendChild(a);
      a.click();
      a.remove();
      // 延后回收,规避个别环境下 click 触发下载与 revoke 的竞态。
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) {
      uiStore.toast.error('导出失败', e instanceof Error ? e.message : '未知错误');
    }
  }

  function focusComposer() {
    composerRef?.focus();
  }

  async function onAssign() {
    const id = selectedId();
    if (!id) return;
    const who = window.prompt('分派给(输入 admin ID,留空取消分派):', detail()?.item.assigneeAdminId ?? '');
    if (who === null) return;
    setActing(true);
    try {
      await adminApi.assignFeedback(id, who.trim() || null);
      uiStore.toast.success(who.trim() ? '已分派' : '已取消分派');
      refetchDetail();
      refetchStats();
      refetchList();
    } catch (e) {
      uiStore.toast.error('分派失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setActing(false);
    }
  }

  async function onGithubIssue() {
    const id = selectedId();
    if (!id) return;
    setActing(true);
    try {
      const r = await adminApi.createFeedbackGithubIssue(id);
      uiStore.toast.success('已转 GitHub Issue', r.issueUrl);
      refetchDetail();
    } catch (e) {
      if (e instanceof ApiError && e.code === 'GITHUB_NOT_CONFIGURED') {
        uiStore.toast.warning('未配置 GitHub', '后端缺少 GITHUB_TOKEN / FEEDBACK_GITHUB_REPO');
      } else {
        uiStore.toast.error('转 Issue 失败', e instanceof Error ? e.message : '未知错误');
      }
    } finally {
      setActing(false);
    }
  }

  async function onResolve() {
    const id = selectedId();
    if (!id) return;
    setActing(true);
    try {
      await adminApi.resolveFeedback(id, null);
      uiStore.toast.success('已标记为已解决');
      refetchDetail();
      refetchStats();
      refetchList();
    } catch (e) {
      uiStore.toast.error('操作失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setActing(false);
    }
  }

  async function onMerge() {
    const id = selectedId();
    if (!id) return;
    const target = window.prompt('合并到目标工单 ID:');
    if (!target || !target.trim()) return;
    if (target.trim() === id) {
      uiStore.toast.warning('无法合并', '不能合并到工单自身');
      return;
    }
    setActing(true);
    try {
      await adminApi.mergeFeedback(id, target.trim());
      uiStore.toast.success('已合并', `本工单已关闭,合并到 ${target.trim()}`);
      refetchDetail();
      refetchStats();
      refetchList();
    } catch (e) {
      uiStore.toast.error('合并失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setActing(false);
    }
  }

  async function onSendReply() {
    const id = selectedId();
    const body = replyBody().trim();
    if (!id || !body) return;
    setSending(true);
    try {
      await adminApi.createFeedbackReply(id, {
        body,
        pushInapp: pushInapp(),
        ccEmail: ccEmail(),
      });
      uiStore.toast.success('回复已发送');
      setReplyBody('');
      refetchDetail();
      refetchStats();
    } catch (e) {
      uiStore.toast.error('发送失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setSending(false);
    }
  }

  async function onSaveDraft() {
    const id = selectedId();
    if (!id) return;
    setSavingDraft(true);
    try {
      await adminApi.saveFeedbackDraft(id, {
        body: replyBody(),
        pushInapp: pushInapp(),
        ccEmail: ccEmail(),
      });
      uiStore.toast.success('已存为草稿', '下次打开此工单将自动恢复');
    } catch (e) {
      uiStore.toast.error('保存草稿失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setSavingDraft(false);
    }
  }

  function insertSnippet(text: string) {
    setReplyBody((prev) => (prev ? `${prev}\n${text}` : text));
    focusComposer();
  }

  // 工具条:在光标选区两侧包裹标记(粗体 / 斜体 / 行内代码),无选区则插入占位
  function wrapSelection(before: string, after: string, placeholder: string) {
    const ta = composerRef;
    if (!ta) {
      setReplyBody((p) => `${p}${before}${placeholder}${after}`);
      return;
    }
    const start = ta.selectionStart;
    const end = ta.selectionEnd;
    const val = ta.value;
    const sel = val.slice(start, end) || placeholder;
    const next = val.slice(0, start) + before + sel + after + val.slice(end);
    setReplyBody(next);
    // 还原选区到被包裹文本上
    queueMicrotask(() => {
      ta.focus();
      ta.setSelectionRange(start + before.length, start + before.length + sel.length);
    });
  }

  // @提及:在光标处插入 @,无选区直接补 @
  function insertAtCursor(text: string) {
    const ta = composerRef;
    if (!ta) {
      setReplyBody((p) => `${p}${text}`);
      return;
    }
    const start = ta.selectionStart;
    const val = ta.value;
    setReplyBody(val.slice(0, start) + text + val.slice(start));
    queueMicrotask(() => {
      ta.focus();
      ta.setSelectionRange(start + text.length, start + text.length);
    });
  }

  function onComposerKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      onSendReply();
    }
  }

  function onCategoryTab(v: string) {
    batch(() => {
      setCategory(v);
      setSelectedId(null);
    });
  }

  function onFilter(f: FilterKey) {
    batch(() => {
      setFilter(f);
      setSelectedId(null);
    });
  }

  return (
    <div class="space-y-5">
      <PageHeader
        title="反馈中心"
        desc="来自客户端 / 网页的统一收件箱。可一键回复、分派、转 GitHub Issue、合并重复。SLA 计时 30 分钟。"
        actions={
          <>
            <Button variant="secondary" size="sm" onClick={onMarkAllRead}>
              标记全部已读
            </Button>
            <Button variant="secondary" size="sm" onClick={onExportCsv}>
              导出 CSV
            </Button>
            <Button variant="primary" size="sm" onClick={() => setAnnOpen(true)}>
              新建公告 / FAQ
            </Button>
          </>
        }
      />

      {/* ── KPI 条 ── */}
      {/* stats 进 error 态时整段降级为提示,避免卡内裸读 stats() 重抛冒泡到全局 ErrorBoundary */}
      <Show
        when={!stats.error}
        fallback={
          <div class="flex items-center gap-3 py-3 text-sm text-content-tertiary">
            统计加载失败,请稍后
            <Button variant="ghost" size="sm" onClick={() => refetchStats()}>重试</Button>
          </div>
        }
      >
      <div class="fb-summary animate-fade-in-up">
        {/* 未处理 + 分类构成 */}
        <div class="fb-inbox-card">
          <div class="lbl">未处理</div>
          <div class="v">
            <Show when={stats()} fallback="—">
              {stats()!.unresolved}
            </Show>
            <span class="unit">/ {stats()?.total ?? 0} 总数</span>
          </div>
          <Show when={stats() && stats()!.unresolved > 0}>
            <div class="breakdown" aria-label="按类别构成">
              <For
                each={[
                  { k: 'bug', color: 'var(--error)', label: 'Bug' },
                  { k: 'feature', color: 'var(--info)', label: '建议' },
                  { k: 'complaint', color: 'var(--warning)', label: '投诉' },
                  { k: 'praise', color: 'var(--success)', label: '表扬' },
                ]}
              >
                {(seg) => (
                  <Show when={catCount(seg.k) > 0}>
                    <span
                      style={{ flex: catCount(seg.k), background: seg.color }}
                      title={`${seg.label} ${catCount(seg.k)}`}
                    />
                  </Show>
                )}
              </For>
            </div>
          </Show>
          <div class="legend">
            <For
              each={[
                { k: 'bug', color: 'var(--error)', label: 'Bug' },
                { k: 'feature', color: 'var(--info)', label: '建议' },
                { k: 'complaint', color: 'var(--warning)', label: '投诉' },
                { k: 'praise', color: 'var(--success)', label: '表扬' },
              ]}
            >
              {(seg) => (
                <span>
                  <span class="swatch" style={{ background: seg.color }} />
                  {seg.label} · {catCount(seg.k)}
                </span>
              )}
            </For>
          </div>
        </div>

        {/* 中位响应时长 —— 数值降为好(down=改善) */}
        <div class="kpi is-success">
          <div class="kpi-label">中位响应时长</div>
          <div class="kpi-value">
            <Show when={stats() && stats()!.medianResponseMinutes != null} fallback="—">
              {Math.round(stats()!.medianResponseMinutes!)}
              <span class="unit">分钟</span>
            </Show>
          </div>
          <div class="kpi-trend">
            {deltaBadge(stats()?.responseDelta ?? null, { unit: '分', goodWhenDown: true, round: true })}
            <span>比上周</span>
          </div>
          <Show when={(stats()?.responseSpark?.length ?? 0) >= 2}>
            <Sparkline class="kpi-spark" data={stats()!.responseSpark} stroke="var(--success)" ariaLabel="近 7 日响应时长走势" />
          </Show>
        </div>

        {/* 7 日解决率 —— 数值升为好(up=改善) */}
        <div class="kpi is-accent">
          <div class="kpi-label">7 日解决率</div>
          <div class="kpi-value">
            <Show when={stats() && stats()!.resolveRate7d != null} fallback="—">
              {(stats()!.resolveRate7d! * 100).toFixed(1)}
              <span class="unit">%</span>
            </Show>
          </div>
          <div class="kpi-trend">
            {deltaBadge(stats()?.resolveDelta ?? null, { unit: '%', goodWhenDown: false, pct: true })}
            <span>SLA 达标</span>
          </div>
          <Show when={(stats()?.resolveSpark?.length ?? 0) >= 2}>
            <Sparkline class="kpi-spark" data={stats()!.resolveSpark} stroke="var(--accent)" ariaLabel="近 7 日解决率走势" />
          </Show>
        </div>

        {/* CSAT —— 数值升为好 */}
        <div class="kpi is-info">
          <div class="kpi-label">CSAT 评分</div>
          <div class="kpi-value">
            <Show when={stats() && stats()!.csatAvg != null} fallback="—">
              {stats()!.csatAvg!.toFixed(1)}
              <span class="unit">/ 5 · 样本 {stats()!.csatCount}</span>
            </Show>
          </div>
          <div class="kpi-trend">
            {deltaBadge(stats()?.csatDelta ?? null, { unit: '', goodWhenDown: false, fixed: 1 })}
            <span>比上周</span>
          </div>
          <Show when={(stats()?.csatSpark?.length ?? 0) >= 2}>
            <Sparkline class="kpi-spark" data={stats()!.csatSpark} stroke="var(--info)" ariaLabel="近 7 日 CSAT 走势" />
          </Show>
        </div>
      </div>
      </Show>

      {/* ── 分类 tab + 筛选 ── */}
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="fb-tabs" role="tablist" aria-label="按分类筛选">
          <For each={CAT_TABS}>
            {(t) => (
              <button
                type="button"
                role="tab"
                aria-selected={category() === t.value}
                class={`fb-tab ${category() === t.value ? 'is-active' : ''}`}
                onClick={() => onCategoryTab(t.value)}
              >
                <span class="ic" style={{ background: t.icBg, color: t.icFg }}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d={t.icon} />
                  </svg>
                </span>
                {t.label} <span class="cnt">{catCount(t.value)}</span>
              </button>
            )}
          </For>
        </div>

        <div class="flex items-center gap-2">
        <div
          class="inline-flex rounded-md border border-border-hairline overflow-hidden"
          role="group"
          aria-label="按状态筛选"
        >
          <For
            each={[
              { k: 'unread' as FilterKey, label: '未读', n: () => (stats.error ? undefined : stats()?.unreadCount) },
              { k: 'assigned' as FilterKey, label: '已分派', n: () => (stats.error ? undefined : stats()?.assignedCount) },
              { k: 'resolved' as FilterKey, label: '已解决', n: () => (stats.error ? undefined : stats()?.resolvedCount) },
              { k: 'all' as FilterKey, label: '全部', n: () => (stats.error ? undefined : stats()?.total) },
            ]}
          >
            {(b) => (
              <button
                type="button"
                aria-pressed={filter() === b.k}
                onClick={() => onFilter(b.k)}
                class={`px-3 py-1.5 text-[12px] font-medium border-r border-border-hairline last:border-r-0 transition-colors ${
                  filter() === b.k
                    ? 'bg-accent text-white'
                    : 'bg-surface-secondary text-content-secondary hover:bg-surface-tertiary'
                }`}
              >
                {b.label}
                <Show when={b.n() != null}>
                  <span class="ml-1 tabular-nums opacity-80">{b.n()}</span>
                </Show>
              </button>
            )}
          </For>
        </div>
        <Button
          variant={advActive() ? 'primary' : 'secondary'}
          size="sm"
          aria-expanded={advOpen()}
          onClick={() => setAdvOpen((v) => !v)}
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3" />
          </svg>
          高级筛选
          <Show when={advActive()}>
            <span class="ml-1 tabular-nums opacity-90">·已启用</span>
          </Show>
        </Button>
        </div>
      </div>

      {/* ── 高级筛选面板(平台 / 版本 / 处理人 / 时间) ── */}
      <Show when={advOpen()}>
        <div class="fb-filter-panel animate-fade-in-up">
          <div class="fld">
            <span class="l">平台</span>
            <Select
              value={fPlatform()}
              onChange={(e) => setFPlatform(e.currentTarget.value)}
              options={[{ value: '', label: '全部平台' }, ...platformOptions().map((p) => ({ value: p, label: p }))]}
            />
          </div>
          <div class="fld">
            <span class="l">应用版本</span>
            <Select
              value={fVersion()}
              onChange={(e) => setFVersion(e.currentTarget.value)}
              options={[{ value: '', label: '全部版本' }, ...versionOptions().map((v) => ({ value: v, label: v }))]}
            />
          </div>
          <div class="fld">
            <span class="l">处理人</span>
            <Select
              value={fAssignee()}
              onChange={(e) => setFAssignee(e.currentTarget.value)}
              options={[
                { value: '', label: '全部' },
                { value: '__none__', label: '未分派' },
                ...assigneeOptions().map((a) => ({ value: a, label: `@${a}` })),
              ]}
            />
          </div>
          <div class="fld">
            <span class="l">起始日期(含)</span>
            <Input type="date" value={fSince()} onInput={(e) => setFSince(e.currentTarget.value)} aria-label="起始日期" />
          </div>
          <div class="acts">
            <Button variant="ghost" size="sm" disabled={!advActive()} onClick={clearAdvanced}>
              清除筛选
            </Button>
          </div>
        </div>
      </Show>

      {/* ── 列表 + 详情 ── */}
      <div class="fb-split animate-fade-in-up">
        {/* 左:列表 */}
        <div class="fb-list">
          <div class="fb-list-head">
            <div class="input-wrap">
              <Input
                value={search()}
                onInput={(e) => setSearch(e.currentTarget.value)}
                placeholder="搜索主题、用户、错误码…"
                aria-label="搜索反馈"
                icon={
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <circle cx="11" cy="11" r="7" />
                    <path d="m21 21-4.3-4.3" />
                  </svg>
                }
              />
            </div>
            <Button
              variant="ghost"
              size="sm"
              loading={listResp.loading}
              onClick={() => {
                refetchList();
                refetchStats();
              }}
              aria-label="刷新列表"
            >
              刷新
            </Button>
          </div>

          <div class="fb-list-rows">
            <Show
              when={!listResp.loading && !listResp.error}
              fallback={
                <Show
                  when={listResp.error}
                  fallback={
                    <div class="py-12 flex justify-center">
                      <Spinner size="lg" />
                    </div>
                  }
                >
                  <div class="py-12 flex flex-col items-center gap-3">
                    <Empty title="列表加载失败" description="后端接口暂时不可用,请重试" />
                    <Button variant="secondary" size="sm" onClick={() => refetchList()}>重试</Button>
                  </div>
                </Show>
              }
            >
              <Show
                when={rows().length > 0}
                fallback={
                  <Empty
                    title="暂无反馈"
                    description={search() ? '没有匹配搜索词的记录' : '此筛选条件下没有反馈'}
                  />
                }
              >
                <For each={rows()}>
                  {(it) => (
                    <div
                      class={`fb-row ${it.readAt == null ? 'is-unread' : ''} ${
                        selectedId() === it.id ? 'is-selected' : ''
                      }`}
                      role="button"
                      tabindex="0"
                      aria-pressed={selectedId() === it.id}
                      onClick={() => selectRow(it)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          selectRow(it);
                        }
                      }}
                    >
                      <div class="unread-dot" />
                      <div class="avatar" style={{ background: avatarGrad(it.category) }}>
                        {avatarChar(it)}
                      </div>
                      <div class="body">
                        <div class="meta">
                          <span class="name">
                            {it.userName || it.userId}
                            <Show when={it.platform}> · {it.platform}</Show>
                            <Show when={it.appVersion}> {it.appVersion}</Show>
                          </span>
                          <Show when={it.category}>
                            <span class={`cat-badge ${catClass(it.category)}`}>{catLabel(it.category)}</span>
                          </Show>
                          <span class={`pri-badge ${priClass(it.priority)}`}>{priClass(it.priority)}</span>
                        </div>
                        <div class="subject">{subjectOf(it.body)}</div>
                        <div class="preview">{previewOf(it.body)}</div>
                      </div>
                      <div class="right">
                        <span class="time">{formatRelativeTime(it.createdAt)}</span>
                        <Show when={rowChip(it)}>
                          {(c) => <span class={`chip ${c().cls}`}>{c().label}</span>}
                        </Show>
                        <Show when={it.dedupCount > 0}>
                          <span class="cat-badge support">×{it.dedupCount + 1}</span>
                        </Show>
                      </div>
                    </div>
                  )}
                </For>
              </Show>
            </Show>
          </div>
        </div>

        {/* 右:详情 */}
        <div class="fb-detail">
          <Show
            when={selectedId()}
            fallback={
              <div class="flex-1 grid place-items-center p-10">
                <Empty
                  title="选择一条反馈查看详情"
                  description="点击左侧任一工单,查看上下文、时间线并回复用户"
                />
              </div>
            }
          >
            <Show
              when={!detail.loading && !detail.error && detail()}
              fallback={
                <Show
                  when={detail.error}
                  fallback={
                    <div class="flex-1 grid place-items-center p-10">
                      <Spinner size="lg" />
                    </div>
                  }
                >
                  <div class="flex-1 grid place-items-center p-10">
                    <div class="flex flex-col items-center gap-3">
                      <Empty title="详情加载失败" description="该工单详情接口暂时不可用,请重试" />
                      <Button variant="secondary" size="sm" onClick={() => refetchDetail()}>重试</Button>
                    </div>
                  </div>
                </Show>
              }
            >
              {(d) => {
                const it = () => d().item;
                return (
                  <>
                    <div class="fb-detail-head">
                      <div class="breadcrumb-mini">
                        <span>#{it().id.slice(0, 8)}</span>
                        <Show when={it().platform}>
                          <span aria-hidden="true">›</span>
                          <span>
                            {it().platform}
                            <Show when={it().appVersion}> / {it().appVersion}</Show>
                          </span>
                        </Show>
                        <span aria-hidden="true">›</span>
                        <Show when={it().category}>
                          <span class={`cat-badge ${catClass(it().category)}`}>{catLabel(it().category)} 报告</span>
                        </Show>
                        <span class={`pri-badge ${priClass(it().priority)}`}>{priClass(it().priority)}</span>
                      </div>
                      <h2>{subjectOf(it().body)}</h2>
                      <div class="meta-row">
                        <span class="reporter">
                          <span class="av">{avatarChar(it())}</span>
                          <strong>{it().userName || '匿名用户'}</strong> · {it().userId}
                          <Show when={it().platform}> · {it().platform} 端</Show>
                        </span>
                        <span aria-hidden="true">·</span>
                        <span>提交于 {formatRelativeTime(it().createdAt)}</span>
                        <Show when={it().status !== 'resolved' && it().status !== 'closed'}>
                          <span aria-hidden="true">·</span>
                          <span>
                            SLA 剩余{' '}
                            <strong style={{ color: slaRemaining(it().createdAt) <= 5 ? 'var(--error-strong)' : 'var(--content)' }}>
                              {slaRemaining(it().createdAt)} 分
                            </strong>
                          </span>
                        </Show>
                        <Show when={it().dedupCount > 0}>
                          <span aria-hidden="true">·</span>
                          <span>
                            已重复 <strong>{it().dedupCount}</strong> 次(去重合并)
                          </span>
                        </Show>
                      </div>
                      <div class="fb-detail-actions">
                        <Button variant="primary" size="sm" onClick={focusComposer}>
                          回复用户
                        </Button>
                        <Button variant="secondary" size="sm" loading={acting()} onClick={onAssign}>
                          {it().assigneeAdminId ? `已分派 @${it().assigneeAdminId}` : '分派给'}
                        </Button>
                        <Button variant="secondary" size="sm" loading={acting()} onClick={onGithubIssue}>
                          <Show when={it().githubIssueUrl} fallback="转 GitHub Issue">
                            已转 Issue
                          </Show>
                        </Button>
                        <Button
                          variant="success"
                          size="sm"
                          loading={acting()}
                          disabled={it().status === 'resolved' || it().status === 'closed'}
                          onClick={onResolve}
                        >
                          标为已解决
                        </Button>
                        <Button variant="ghost" size="sm" loading={acting()} onClick={onMerge}>
                          合并重复
                        </Button>
                      </div>
                    </div>

                    {/* 上下文四格 */}
                    <div class="fb-context">
                      <div>
                        <div class="l">应用版本</div>
                        <div class="v">{it().appVersion ?? '—'}</div>
                      </div>
                      <div>
                        <div class="l">系统</div>
                        <div class="v">
                          {[it().platform, (it().deviceProfile?.osName as string) ?? null]
                            .filter(Boolean)
                            .join(' · ') || '—'}
                        </div>
                      </div>
                      <div>
                        <div class="l">网络</div>
                        <div class="v">{(it().deviceProfile?.network as string) ?? '—'}</div>
                      </div>
                      <div>
                        <div class="l">推送通道</div>
                        <div class="v">{(it().deviceProfile?.push as string) ?? '—'}</div>
                      </div>
                    </div>

                    {/* 正文 + 附件 */}
                    <div class="fb-content">
                      <Show
                        when={looksLikeCode(it().body)}
                        fallback={
                          <For each={it().body.split('\n').filter((l) => l.trim())}>
                            {(line) => <p>{line}</p>}
                          </For>
                        }
                      >
                        <pre>{it().body}</pre>
                      </Show>

                      <Show when={d().attachments.length > 0}>
                        <div class="fb-screenshots">
                          <For each={d().attachments}>
                            {(att) => (
                              <a
                                class="sc"
                                href={att.url}
                                target="_blank"
                                rel="noopener noreferrer"
                                style={
                                  att.kind === 'image'
                                    ? { 'background-image': `url(${att.url})`, 'background-size': 'cover' }
                                    : undefined
                                }
                              >
                                <span class="label">{att.name}</span>
                              </a>
                            )}
                          </For>
                        </div>
                      </Show>
                    </div>

                    {/* 时间线 */}
                    <div class="fb-timeline">
                      <h3>处理时间线</h3>
                      <Show
                        when={d().events.length > 0}
                        fallback={<p class="text-sm text-content-tertiary">暂无处理记录</p>}
                      >
                        <For each={d().events}>
                          {(ev: FeedbackEvent) => {
                            // reply 事件:从 replies 找对应正文展开
                            const reply = () =>
                              ev.kind === 'reply' && ev.refId
                                ? d().replies.find((r) => r.id === ev.refId)
                                : undefined;
                            return (
                              <div class={`fb-event ${eventClass(ev.kind)}`}>
                                <div class="dot">
                                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                    <circle cx="12" cy="12" r="10" />
                                    <path d="M12 6v6l4 2" />
                                  </svg>
                                </div>
                                <div class="body">
                                  <div class="who">{ev.actor ?? '系统'}</div>
                                  <div class="what">{ev.summary}</div>
                                  <Show when={reply()}>
                                    <div class="reply">{reply()!.body}</div>
                                  </Show>
                                  <div class="when">{formatRelativeTime(ev.createdAt)}</div>
                                </div>
                              </div>
                            );
                          }}
                        </For>
                      </Show>
                    </div>

                    {/* 回复 composer */}
                    <div class="fb-composer">
                      {/* 富文本工具条(Markdown 包裹 / @提及;附件因回复端点不支持故省略) */}
                      <div class="toolbar" role="toolbar" aria-label="格式工具">
                        <button type="button" class="tb-btn" title="粗体" aria-label="粗体" onClick={() => wrapSelection('**', '**', '粗体')}>
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <path d="M6 4h8a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" />
                            <path d="M6 12h9a4 4 0 0 1 4 4 4 4 0 0 1-4 4H6z" />
                          </svg>
                        </button>
                        <button type="button" class="tb-btn" title="斜体" aria-label="斜体" onClick={() => wrapSelection('*', '*', '斜体')}>
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <line x1="19" y1="4" x2="10" y2="4" />
                            <line x1="14" y1="20" x2="5" y2="20" />
                            <line x1="15" y1="4" x2="9" y2="20" />
                          </svg>
                        </button>
                        <button type="button" class="tb-btn" title="行内代码" aria-label="行内代码" onClick={() => wrapSelection('`', '`', 'code')}>
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <polyline points="16 18 22 12 16 6" />
                            <polyline points="8 6 2 12 8 18" />
                          </svg>
                        </button>
                        <div class="tb-divider" />
                        <button type="button" class="tb-btn" title="@提及用户" aria-label="@提及用户" onClick={() => insertAtCursor(`@${it().userName || it().userId} `)}>
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <circle cx="12" cy="12" r="4" />
                            <path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-3.92 7.94" />
                          </svg>
                        </button>
                      </div>
                      <div class="snippets">
                        <For each={SNIPPETS}>
                          {(s) => (
                            <button type="button" class="snippet" onClick={() => insertSnippet(s)}>
                              + {s.length > 14 ? `${s.slice(0, 14)}…` : s}
                            </button>
                          )}
                        </For>
                      </div>
                      <textarea
                        ref={composerRef}
                        class="w-full mt-3 px-3 py-2 rounded-md border border-border bg-surface text-[13px] resize-y focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/30 transition"
                        rows={3}
                        value={replyBody()}
                        onInput={(e) => setReplyBody(e.currentTarget.value)}
                        onKeyDown={onComposerKeydown}
                        placeholder={`给 @${it().userName || it().userId} 写一条回复… ⌘+Enter 发送`}
                        aria-label="回复内容"
                      />
                      <div class="send-row">
                        <div class="opts">
                          <Switch checked={pushInapp()} onChange={setPushInapp} label="推送应用内通知" />
                          <Switch checked={ccEmail()} onChange={setCcEmail} label="抄送邮箱" />
                        </div>
                        <div class="flex items-center gap-2">
                          <Button
                            variant="ghost"
                            size="sm"
                            loading={savingDraft()}
                            disabled={!replyBody().trim()}
                            onClick={onSaveDraft}
                          >
                            存为草稿
                          </Button>
                          <Button
                            variant="primary"
                            size="sm"
                            loading={sending()}
                            disabled={!replyBody().trim()}
                            onClick={onSendReply}
                          >
                            发送回复
                          </Button>
                        </div>
                      </div>
                    </div>
                  </>
                );
              }}
            </Show>
          </Show>
        </div>
      </div>

      <AnnouncementManager open={annOpen()} onClose={() => setAnnOpen(false)} />
    </div>
  );
}
