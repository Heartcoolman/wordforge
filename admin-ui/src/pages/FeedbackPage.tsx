import {
  createSignal,
  createResource,
  createMemo,
  createEffect,
  For,
  Show,
  batch,
  onCleanup,
} from 'solid-js';
import {
  PageHead,
  StatCard,
  Card,
  Panel,
  Badge,
  Btn,
  IconBtn,
  Seg,
  Switch,
  Field,
  Empty,
  Skel,
  Loading,
  Spinner,
  Modal,
  Confirm,
  Pagination,
  fmtNum,
  fmtTime,
  fmtAgo,
  toast,
} from '@/components/wf';
import { sx } from '@/components/wf';
import { adminApi } from '@/api/admin';
import { ApiError } from '@/api/http';
import type {
  FeedbackItem,
  FeedbackStats,
  FeedbackDetail,
  FeedbackEvent,
} from '@/types/admin';
import type { AnnouncementKind, FeedbackAnnouncement } from '@/api/admin';

/* ============================================================
   用户反馈 (feedback) — 工单化收件箱
   分派 / 回复 / 解决 / 合并 / 转 GitHub Issue + 公告 / FAQ 管理
   ============================================================ */

type BadgeVariant = 'default' | 'accent' | 'success' | 'warning' | 'error' | 'info';

// 状态 → [中文, Badge 变体]
const FB_STATUS: Record<string, [string, BadgeVariant]> = {
  open: ['待处理', 'warning'],
  in_progress: ['处理中', 'info'],
  resolved: ['已解决', 'success'],
  closed: ['已关闭', 'default'],
};
// 优先级 → [中文, Badge 变体]
const FB_PRIORITY: Record<string, [string, BadgeVariant]> = {
  low: ['低', 'default'],
  normal: ['中', 'info'],
  medium: ['中', 'info'],
  high: ['高', 'warning'],
  urgent: ['紧急', 'error'],
};
// 分类 → 中文
const FB_CAT: Record<string, string> = {
  bug: 'Bug',
  feature: '功能请求',
  complaint: '投诉',
  complain: '投诉',
  praise: '表扬',
  support: '咨询',
  other: '其他',
};
// 时间线事件 kind → 中文
const EVENT_LABEL: Record<string, string> = {
  submitted: '用户提交',
  created: '用户提交',
  assigned: '已分派',
  reply: '已回复',
  replied: '已回复',
  resolved: '已解决',
  merged: '已合并',
  github: '转 Issue',
};

// 回复快捷文案
const SNIPPETS = [
  '已修复，将在下个版本上线。',
  '已转研发跟进。',
  '感谢反馈，能否补充复现步骤 / 截图？',
  '感谢你的支持与肯定！',
];

const PER_PAGE = 30;

function statusMeta(s: string): [string, BadgeVariant] {
  return FB_STATUS[s] ?? [s, 'default'];
}
function priMeta(p: string): [string, BadgeVariant] {
  return FB_PRIORITY[p] ?? [p, 'default'];
}
function catLabel(c: string | null | undefined): string {
  return c ? FB_CAT[c] ?? c : '其他';
}
// body 首行作为主题
function subjectOf(body: string): string {
  return (body || '').split('\n')[0].trim() || '(无内容)';
}

type StatusFilter = 'all' | 'open' | 'in_progress' | 'resolved';

export default function FeedbackPage() {
  // ── 列表筛选状态 ──
  const [status, setStatus] = createSignal<StatusFilter>('all');
  const [category, setCategory] = createSignal('');
  const [unread, setUnread] = createSignal(false);
  const [assigned, setAssigned] = createSignal(false);
  const [page, setPage] = createSignal(1);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);

  // ── 公告 / FAQ 弹窗 ──
  const [annOpen, setAnnOpen] = createSignal(false);

  // ── 全部已读确认 ──
  const [markAllOpen, setMarkAllOpen] = createSignal(false);
  const [marking, setMarking] = createSignal(false);
  const [exporting, setExporting] = createSignal(false);

  // ── KPI ──
  const [stats, { refetch: refetchStats }] = createResource<FeedbackStats>(() =>
    adminApi.getFeedbackStats(),
  );

  // ── 列表（随筛选 / 分页变化重取）──
  const listKey = createMemo(() => ({
    status: status(),
    category: category(),
    unread: unread(),
    assigned: assigned(),
    page: page(),
  }));
  const [listResp, { refetch: refetchList }] = createResource(listKey, async (k) => {
    const params: Parameters<typeof adminApi.listFeedback>[0] = {
      page: k.page,
      perPage: PER_PAGE,
    };
    if (k.category) params.category = k.category;
    if (k.status !== 'all') params.status = k.status;
    if (k.unread) params.unread = true;
    if (k.assigned) params.assigned = true;
    return adminApi.listFeedback(params);
  });
  const rows = createMemo<FeedbackItem[]>(() => listResp()?.data ?? []);
  const totalPages = createMemo(() => listResp()?.totalPages ?? 1);

  // ── 详情（随 selectedId 变化重取，GET 会置 readAt）──
  const [detail, { refetch: refetchDetail }] = createResource<FeedbackDetail | null, string | null>(
    selectedId,
    async (id) => (id ? adminApi.getFeedbackDetail(id) : null),
  );

  function reloadAll() {
    refetchList();
    refetchStats();
  }

  // ── 筛选切换：重置分页 + 清空选中 ──
  function changeStatus(v: string) {
    batch(() => {
      setStatus(v as StatusFilter);
      setPage(1);
    });
  }
  function changeCategory(v: string) {
    batch(() => {
      setCategory(v);
      setPage(1);
    });
  }
  function toggleUnread(v: boolean) {
    batch(() => {
      setUnread(v);
      setPage(1);
    });
  }
  function toggleAssigned(v: boolean) {
    batch(() => {
      setAssigned(v);
      setPage(1);
    });
  }

  // ── 全部已读 ──
  async function onMarkAllRead() {
    setMarking(true);
    try {
      const r = await adminApi.markAllFeedbackRead();
      toast.success('已全部标记已读', `更新 ${r.updated} 条`);
      setMarkAllOpen(false);
      reloadAll();
    } catch (e) {
      toast.error('操作失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setMarking(false);
    }
  }

  // ── 导出 CSV（raw-fetch ObjectURL）──
  async function onExportCsv() {
    setExporting(true);
    try {
      const url = await adminApi.feedbackCsvUrl();
      const a = document.createElement('a');
      a.href = url;
      a.download = 'feedback.csv';
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      toast.success('已导出反馈 CSV');
    } catch (e) {
      toast.error('导出失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setExporting(false);
    }
  }

  return (
    <div>
      <PageHead
        title="用户反馈"
        desc="工单化的反馈中心：分派、回复、解决、合并、转 GitHub Issue，并管理公告 / FAQ。"
        right={
          <>
            <Btn variant="secondary" icon="check" onClick={() => setMarkAllOpen(true)}>
              全部已读
            </Btn>
            <Btn variant="secondary" icon="msg" onClick={() => setAnnOpen(true)}>
              公告 / FAQ
            </Btn>
            <Btn variant="secondary" icon="download" onClick={onExportCsv} disabled={exporting()}>
              {exporting() ? '导出中…' : '导出'}
            </Btn>
          </>
        }
      />

      {/* ── KPI 卡 ── */}
      <Show
        when={!stats.error}
        fallback={
          <Card style={sx({ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16 })}>
            <span class="muted">统计加载失败</span>
            <Btn size="sm" variant="ghost" icon="refresh" onClick={() => refetchStats()}>
              重试
            </Btn>
          </Card>
        }
      >
        <Show
          when={stats()}
          fallback={
            <div style={statGridStyle()}>
              <For each={[0, 1, 2, 3, 4]}>{() => <Skel h={116} r={16} />}</For>
            </div>
          }
        >
          {(s) => (
            <div style={statGridStyle()}>
              <StatCard
                tone="warning"
                label="待处理"
                icon="inbox"
                value={fmtNum(s().byStatus?.open ?? s().unresolved)}
              />
              <StatCard
                tone="info"
                label="处理中"
                icon="clock"
                value={fmtNum(s().byStatus?.inProgress ?? 0)}
              />
              <StatCard
                tone="success"
                label="已解决"
                icon="check"
                value={fmtNum(s().resolvedCount ?? s().byStatus?.resolved ?? 0)}
              />
              <StatCard tone="accent" label="未读" icon="bell" value={fmtNum(s().unreadCount)} />
              <StatCard
                tone="error"
                label="中位响应"
                icon="clock"
                value={
                  s().medianResponseMinutes != null
                    ? Math.round(s().medianResponseMinutes!)
                    : '—'
                }
                unit={s().medianResponseMinutes != null ? '分' : undefined}
                spark={(s().responseSpark?.length ?? 0) > 1 ? s().responseSpark : undefined}
              />
            </div>
          )}
        </Show>
      </Show>

      {/* ── 列表 + 详情 ── */}
      <div style={sx({ display: 'grid', gridTemplateColumns: '380px 1fr', gap: 16 })} class="grid-collapse">
        {/* 左：收件箱 */}
        <Card pad={false} style={sx({ overflow: 'hidden', alignSelf: 'start' })}>
          <div style={sx({ padding: 12, borderBottom: '1px solid var(--hairline)' })}>
            <Seg
              options={[
                { value: 'all', label: '全部' },
                { value: 'open', label: '待处理' },
                { value: 'in_progress', label: '处理中' },
                { value: 'resolved', label: '已解决' },
              ]}
              value={status()}
              onChange={changeStatus}
            />
          </div>
          <div
            style={sx({
              padding: '8px 12px',
              borderBottom: '1px solid var(--hairline)',
              display: 'flex',
              gap: 8,
              alignItems: 'center',
              flexWrap: 'wrap',
            })}
          >
            <select
              class="select"
              style={sx({ width: 'auto', padding: '5px 9px', fontSize: 12 })}
              value={category()}
              onChange={(e) => changeCategory(e.currentTarget.value)}
            >
              <option value="">全部分类</option>
              <option value="bug">Bug</option>
              <option value="feature">功能请求</option>
              <option value="complaint">投诉</option>
              <option value="praise">表扬</option>
              <option value="support">咨询</option>
            </select>
            <label
              style={sx({ display: 'flex', gap: 6, alignItems: 'center', fontSize: 12, marginLeft: 'auto' })}
            >
              <Switch checked={assigned()} onChange={toggleAssigned} />
              仅已分派
            </label>
            <label style={sx({ display: 'flex', gap: 6, alignItems: 'center', fontSize: 12 })}>
              <Switch checked={unread()} onChange={toggleUnread} />
              仅未读
            </label>
          </div>

          <div style={sx({ maxHeight: 560, overflowY: 'auto' })}>
            <Show
              when={!listResp.loading && !listResp.error}
              fallback={
                <Show
                  when={listResp.error}
                  fallback={
                    <div style={sx({ padding: 14, display: 'flex', flexDirection: 'column', gap: 8 })}>
                      <Skel h={64} />
                      <Skel h={64} />
                      <Skel h={64} />
                    </div>
                  }
                >
                  <div style={sx({ padding: '24px 14px' })}>
                    <Empty title="列表加载失败" desc="后端接口暂时不可用" icon="alert" />
                    <div style={sx({ display: 'flex', justifyContent: 'center', marginTop: 10 })}>
                      <Btn size="sm" variant="secondary" icon="refresh" onClick={() => refetchList()}>
                        重试
                      </Btn>
                    </div>
                  </div>
                </Show>
              }
            >
              <Show when={rows().length > 0} fallback={<div style={sx({ padding: '24px 14px' })}><Empty title="暂无反馈" icon="inbox" /></div>}>
                <For each={rows()}>
                  {(f) => {
                    const pri = () => priMeta(f.priority);
                    const st = () => statusMeta(f.status);
                    const sel = () => selectedId() === f.id;
                    return (
                      <button
                        onClick={() => setSelectedId(f.id)}
                        style={sx({
                          width: '100%',
                          textAlign: 'left',
                          padding: '12px 14px 12px 18px',
                          border: 'none',
                          borderBottom: '1px solid var(--hairline)',
                          cursor: 'pointer',
                          background: sel() ? 'var(--accent-soft)' : 'transparent',
                          position: 'relative',
                        })}
                      >
                        <Show when={f.readAt == null}>
                          <span
                            style={sx({
                              position: 'absolute',
                              left: 6,
                              top: 17,
                              width: 6,
                              height: 6,
                              borderRadius: 50,
                              background: 'var(--accent)',
                            })}
                          />
                        </Show>
                        <div style={sx({ display: 'flex', gap: 6, alignItems: 'center', marginBottom: 5, flexWrap: 'wrap' })}>
                          <Badge variant={pri()[1]}>{pri()[0]}</Badge>
                          <Show when={f.category}>
                            <Badge variant="default">{catLabel(f.category)}</Badge>
                          </Show>
                          <Badge variant={st()[1]} dot>
                            {st()[0]}
                          </Badge>
                        </div>
                        <div
                          style={sx({
                            fontWeight: 600,
                            fontSize: 13,
                            color: sel() ? 'var(--accent)' : 'var(--text)',
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            whiteSpace: 'nowrap',
                          })}
                        >
                          {subjectOf(f.body)}
                        </div>
                        <div class="muted-3" style={sx({ fontSize: 11, marginTop: 3 })}>
                          {f.userName || f.userId}
                          <Show when={f.platform}> · {f.platform}</Show>
                          <Show when={f.appVersion}> v{f.appVersion}</Show>
                          {' · '}
                          {fmtAgo(f.createdAt)}
                          <Show when={f.dedupCount > 0}> · ×{f.dedupCount + 1}</Show>
                        </div>
                      </button>
                    );
                  }}
                </For>
              </Show>
            </Show>
          </div>

          <Show when={totalPages() > 1}>
            <div style={sx({ padding: '4px 12px 12px' })}>
              <Pagination page={page()} totalPages={totalPages()} onPage={setPage} />
            </div>
          </Show>
        </Card>

        {/* 右：详情 */}
        <Show
          when={selectedId()}
          fallback={
            <Card style={sx({ display: 'grid', placeItems: 'center', minHeight: 360 })}>
              <Empty
                title="选择一条反馈查看详情"
                desc="点击左侧任一工单，查看上下文、时间线并回复用户"
                icon="inbox"
              />
            </Card>
          }
        >
          <FeedbackDetailPanel
            id={selectedId()!}
            detail={detail()}
            loading={detail.loading}
            error={!!detail.error}
            onRefetchDetail={refetchDetail}
            onReload={reloadAll}
          />
        </Show>
      </div>

      <AnnouncementModal open={annOpen()} onClose={() => setAnnOpen(false)} />

      <Confirm
        open={markAllOpen()}
        onClose={() => setMarkAllOpen(false)}
        onConfirm={onMarkAllRead}
        loading={marking()}
        title="全部标记已读？"
        body="将把当前所有未读反馈标记为已读，未读计数清零。"
        confirmText="全部已读"
      />
    </div>
  );
}

function statGridStyle() {
  return sx({
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fit,minmax(160px,1fr))',
    gap: 14,
    marginBottom: 16,
  });
}

/* ---------------------------- 详情面板 ---------------------------- */
function FeedbackDetailPanel(props: {
  id: string;
  detail: FeedbackDetail | null | undefined;
  loading: boolean;
  error: boolean;
  onRefetchDetail: () => void;
  onReload: () => void;
}) {
  // composer 状态
  const [reply, setReply] = createSignal('');
  const [pushInapp, setPushInapp] = createSignal(true);
  const [busy, setBusy] = createSignal<'' | 'reply' | 'draft' | 'assign' | 'resolve' | 'gh'>('');
  const [mergeOpen, setMergeOpen] = createSignal(false);
  const [mergeTarget, setMergeTarget] = createSignal('');
  const [assignOpen, setAssignOpen] = createSignal(false);
  const [assignee, setAssignee] = createSignal('');
  let composerRef: HTMLTextAreaElement | undefined;
  let draftTimer: ReturnType<typeof setTimeout> | undefined;

  const d = () => props.detail;
  const item = () => props.detail?.item;

  // 切换工单：重置 composer，再异步回填草稿
  createEffect(() => {
    const id = props.id;
    batch(() => {
      setReply('');
      setPushInapp(true);
    });
    adminApi
      .getFeedbackDraft(id)
      .then((r) => {
        if (props.id !== id || !r.draft) return;
        batch(() => {
          setReply(r.draft!.body);
          setPushInapp(r.draft!.pushInapp);
        });
      })
      .catch(() => {
        /* 无草稿：保持空白 */
      });
  });

  onCleanup(() => {
    if (draftTimer) clearTimeout(draftTimer);
  });

  // 草稿自动保存（输入停顿 1.2s 后静默 upsert）
  function scheduleDraftSave() {
    if (draftTimer) clearTimeout(draftTimer);
    const id = props.id;
    draftTimer = setTimeout(() => {
      adminApi
        .saveFeedbackDraft(id, { body: reply(), pushInapp: pushInapp() })
        .catch(() => {
          /* 自动保存失败不打扰 */
        });
    }, 1200);
  }

  function onReplyInput(v: string) {
    setReply(v);
    scheduleDraftSave();
  }

  async function sendReply() {
    const body = reply().trim();
    if (!body) return;
    setBusy('reply');
    try {
      await adminApi.createFeedbackReply(props.id, {
        body,
        pushInapp: pushInapp(),
      });
      toast.success('已回复');
      setReply('');
      props.onRefetchDetail();
      props.onReload();
    } catch (e) {
      toast.error('发送失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy('');
    }
  }

  async function saveDraftNow() {
    setBusy('draft');
    try {
      await adminApi.saveFeedbackDraft(props.id, {
        body: reply(),
        pushInapp: pushInapp(),
      });
      toast.success('已存为草稿', '下次打开此工单将自动恢复');
    } catch (e) {
      toast.error('保存草稿失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy('');
    }
  }

  async function resolve() {
    setBusy('resolve');
    try {
      await adminApi.resolveFeedback(props.id, null);
      toast.success('已标记为已解决');
      props.onRefetchDetail();
      props.onReload();
    } catch (e) {
      toast.error('操作失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy('');
    }
  }

  async function doAssign() {
    setBusy('assign');
    try {
      await adminApi.assignFeedback(props.id, assignee().trim() || null);
      toast.success(assignee().trim() ? '已分派' : '已取消分派');
      setAssignOpen(false);
      props.onRefetchDetail();
      props.onReload();
    } catch (e) {
      toast.error('分派失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy('');
    }
  }

  async function doMerge() {
    const target = mergeTarget().trim();
    if (!target) return;
    if (target === props.id) {
      toast.warning('无法合并', '不能合并到工单自身');
      return;
    }
    setBusy('reply');
    try {
      await adminApi.mergeFeedback(props.id, target);
      toast.success('已合并', `本工单已关闭，合并到 ${target}`);
      setMergeOpen(false);
      setMergeTarget('');
      props.onRefetchDetail();
      props.onReload();
    } catch (e) {
      toast.error('合并失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy('');
    }
  }

  async function toGithub() {
    setBusy('gh');
    try {
      const r = await adminApi.createFeedbackGithubIssue(props.id);
      toast.success('已创建 GitHub Issue', r.issueUrl);
      props.onRefetchDetail();
    } catch (e) {
      if (e instanceof ApiError && e.code === 'GITHUB_NOT_CONFIGURED') {
        toast.warning('未配置 GitHub', '后端缺少 GITHUB_TOKEN / FEEDBACK_GITHUB_REPO');
      } else {
        toast.error('转 Issue 失败', e instanceof Error ? e.message : '未知错误');
      }
    } finally {
      setBusy('');
    }
  }

  // composer 工具条：包裹选区
  function wrap(before: string, after: string, placeholder: string) {
    const ta = composerRef;
    if (!ta) {
      onReplyInput(`${reply()}${before}${placeholder}${after}`);
      return;
    }
    const start = ta.selectionStart;
    const end = ta.selectionEnd;
    const val = ta.value;
    const sel = val.slice(start, end) || placeholder;
    onReplyInput(val.slice(0, start) + before + sel + after + val.slice(end));
    queueMicrotask(() => {
      ta.focus();
      ta.setSelectionRange(start + before.length, start + before.length + sel.length);
    });
  }

  function insertSnippet(text: string) {
    onReplyInput(reply() ? `${reply()}\n${text}` : text);
    composerRef?.focus();
  }

  function onComposerKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      sendReply();
    }
  }

  return (
    <Card pad={false} style={sx({ overflow: 'hidden', alignSelf: 'start' })}>
      <Show
        when={!props.loading && !props.error && d()}
        fallback={
          <Show
            when={props.error}
            fallback={<Loading h={400} />}
          >
            <div style={sx({ padding: '36px 20px' })}>
              <Empty title="详情加载失败" desc="该工单详情接口暂时不可用" icon="alert" />
              <div style={sx({ display: 'flex', justifyContent: 'center', marginTop: 10 })}>
                <Btn size="sm" variant="secondary" icon="refresh" onClick={() => props.onRefetchDetail()}>
                  重试
                </Btn>
              </div>
            </div>
          </Show>
        }
      >
        {(detailVal) => {
          const it = () => detailVal().item;
          const pri = () => priMeta(it().priority);
          const st = () => statusMeta(it().status);
          return (
            <>
              {/* 头部 */}
              <div style={sx({ padding: 18, borderBottom: '1px solid var(--hairline)' })}>
                <div style={sx({ display: 'flex', gap: 8, marginBottom: 10, flexWrap: 'wrap' })}>
                  <Badge variant={pri()[1]}>{pri()[0]}优先</Badge>
                  <Show when={it().category}>
                    <Badge variant="default">{catLabel(it().category)}</Badge>
                  </Show>
                  <Badge variant={st()[1]} dot>
                    {st()[0]}
                  </Badge>
                  <Show when={it().dedupCount > 0}>
                    <Badge variant="warning">合并 ×{it().dedupCount + 1}</Badge>
                  </Show>
                  <Show when={it().githubIssueUrl}>
                    <Badge variant="info">已转 Issue</Badge>
                  </Show>
                </div>
                <h2 style={sx({ margin: 0, fontSize: 17, fontWeight: 700 })}>{subjectOf(it().body)}</h2>
                <p
                  class="muted"
                  style={sx({ fontSize: 13, marginTop: 8, lineHeight: 1.6, whiteSpace: 'pre-wrap' })}
                >
                  {it().body}
                </p>
                <div class="muted-3 mono" style={sx({ fontSize: 11.5, marginTop: 8 })}>
                  <Show when={it().platform}>{it().platform}{' '}</Show>
                  <Show when={it().appVersion}>v{it().appVersion} · </Show>
                  用户 {it().userId.slice(0, 12)} · {fmtTime(it().createdAt)}
                  <Show when={it().assigneeAdminId}> · 处理人 @{it().assigneeAdminId}</Show>
                </div>

                {/* 附件 */}
                <Show when={detailVal().attachments.length > 0}>
                  <div style={sx({ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' })}>
                    <For each={detailVal().attachments}>
                      {(att) => (
                        <a
                          href={att.url}
                          target="_blank"
                          rel="noopener noreferrer"
                          class="badge badge-default"
                          style={sx({ textDecoration: 'none', cursor: 'pointer' })}
                        >
                          {att.name}
                        </a>
                      )}
                    </For>
                  </div>
                </Show>

                {/* 操作行 */}
                <div style={sx({ display: 'flex', gap: 7, marginTop: 14, flexWrap: 'wrap' })}>
                  <Btn
                    size="sm"
                    variant="secondary"
                    icon="users"
                    disabled={busy() === 'assign'}
                    onClick={() => {
                      setAssignee(it().assigneeAdminId ?? '');
                      setAssignOpen(true);
                    }}
                  >
                    {it().assigneeAdminId ? `已分派 @${it().assigneeAdminId}` : '分派'}
                  </Btn>
                  <Btn
                    size="sm"
                    variant="success"
                    icon="check"
                    disabled={
                      busy() === 'resolve' || it().status === 'resolved' || it().status === 'closed'
                    }
                    onClick={resolve}
                  >
                    标记已解决
                  </Btn>
                  <Btn
                    size="sm"
                    variant="secondary"
                    icon="layers"
                    onClick={() => setMergeOpen(true)}
                  >
                    合并
                  </Btn>
                  <Btn
                    size="sm"
                    variant="secondary"
                    icon="external"
                    disabled={busy() === 'gh' || !!it().githubIssueUrl}
                    onClick={toGithub}
                  >
                    {busy() === 'gh' ? '创建中…' : it().githubIssueUrl ? '已转 Issue' : '转 Issue'}
                  </Btn>
                </div>
              </div>

              {/* 正文区：时间线 + 回复 + composer */}
              <div style={sx({ padding: 18 })}>
                {/* 时间线 */}
                <div class="eyebrow" style={sx({ marginBottom: 10 })}>
                  处理时间线
                </div>
                <Show
                  when={detailVal().events.length > 0}
                  fallback={<div class="muted-3" style={sx({ fontSize: 12, marginBottom: 16 })}>暂无处理记录</div>}
                >
                  <div style={sx({ position: 'relative', paddingLeft: 18, marginBottom: 18 })}>
                    <div
                      style={sx({
                        position: 'absolute',
                        left: 4,
                        top: 4,
                        bottom: 4,
                        width: 2,
                        background: 'var(--hairline)',
                      })}
                    />
                    <For each={detailVal().events}>
                      {(ev: FeedbackEvent) => (
                        <div style={sx({ position: 'relative', paddingBottom: 14 })}>
                          <span
                            style={sx({
                              position: 'absolute',
                              left: -18,
                              top: 3,
                              width: 9,
                              height: 9,
                              borderRadius: 50,
                              background: 'var(--accent)',
                              border: '2px solid var(--surface-solid)',
                            })}
                          />
                          <div style={sx({ fontSize: 12.5, fontWeight: 600 })}>
                            {EVENT_LABEL[ev.kind] ?? ev.summary ?? ev.kind}
                          </div>
                          <div class="muted-3" style={sx({ fontSize: 11 })}>
                            {ev.actor ?? '系统'} · {fmtAgo(ev.createdAt)}
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>

                {/* 回复列表 */}
                <For each={detailVal().replies}>
                  {(r) => (
                    <div
                      style={sx({
                        padding: 12,
                        borderRadius: 11,
                        background:
                          r.authorKind === 'admin' ? 'var(--accent-soft)' : 'var(--surface-sunken)',
                        marginBottom: 10,
                      })}
                    >
                      <div class="muted-3" style={sx({ fontSize: 11, marginBottom: 4 })}>
                        {r.authorKind === 'admin' ? '管理员' : '用户'}
                        <Show when={r.authorId}> · {r.authorId}</Show> · {fmtAgo(r.createdAt)}
                      </div>
                      <div style={sx({ fontSize: 13, whiteSpace: 'pre-wrap' })}>{r.body}</div>
                    </div>
                  )}
                </For>

                {/* composer */}
                <div
                  style={sx({
                    border: '1px solid var(--border)',
                    borderRadius: 12,
                    overflow: 'hidden',
                    marginTop: 6,
                  })}
                >
                  <div
                    style={sx({
                      display: 'flex',
                      gap: 4,
                      padding: '7px 9px',
                      borderBottom: '1px solid var(--hairline)',
                      background: 'var(--surface-sunken)',
                      flexWrap: 'wrap',
                    })}
                  >
                    <button class="btn btn-ghost btn-xs" title="粗体" onClick={() => wrap('**', '**', '粗体')}>
                      <b>B</b>
                    </button>
                    <button class="btn btn-ghost btn-xs" title="斜体" onClick={() => wrap('*', '*', '斜体')}>
                      <i>I</i>
                    </button>
                    <button class="btn btn-ghost btn-xs" title="行内代码" onClick={() => wrap('`', '`', 'code')}>
                      {'<>'}
                    </button>
                    <div style={sx({ flex: 1 })} />
                    <For each={SNIPPETS}>
                      {(s) => (
                        <button
                          class="btn btn-ghost btn-xs"
                          title={s}
                          onClick={() => insertSnippet(s)}
                        >
                          + {s.length > 10 ? `${s.slice(0, 10)}…` : s}
                        </button>
                      )}
                    </For>
                  </div>
                  <textarea
                    ref={composerRef}
                    class="textarea"
                    rows={3}
                    style={sx({ border: 'none', borderRadius: 0 })}
                    value={reply()}
                    onInput={(e) => onReplyInput(e.currentTarget.value)}
                    onKeyDown={onComposerKeydown}
                    placeholder="回复用户…（支持 Markdown，⌘+Enter 发送，输入自动存草稿）"
                  />
                </div>
                <div
                  style={sx({
                    display: 'flex',
                    alignItems: 'center',
                    gap: 12,
                    marginTop: 10,
                    flexWrap: 'wrap',
                  })}
                >
                  <label style={sx({ display: 'flex', gap: 7, alignItems: 'center', fontSize: 12.5 })}>
                    <Switch checked={pushInapp()} onChange={setPushInapp} />
                    应用内推送
                  </label>
                  <div style={sx({ display: 'flex', gap: 8, marginLeft: 'auto' })}>
                    <Btn
                      variant="ghost"
                      icon="copy"
                      disabled={busy() === 'draft' || !reply().trim()}
                      onClick={saveDraftNow}
                    >
                      {busy() === 'draft' ? '保存中…' : '存草稿'}
                    </Btn>
                    <Btn
                      variant="primary"
                      icon="send"
                      disabled={busy() === 'reply' || !reply().trim()}
                      onClick={sendReply}
                    >
                      {busy() === 'reply' ? '发送中…' : '发送回复'}
                    </Btn>
                  </div>
                </div>
              </div>
            </>
          );
        }}
      </Show>

      {/* 分派弹窗 */}
      <Modal
        open={assignOpen()}
        onClose={() => setAssignOpen(false)}
        title="分派工单"
        size="sm"
        footer={
          <>
            <Btn variant="ghost" onClick={() => setAssignOpen(false)}>
              取消
            </Btn>
            <Btn variant="primary" disabled={busy() === 'assign'} onClick={doAssign}>
              {busy() === 'assign' ? '处理中…' : '确认分派'}
            </Btn>
          </>
        }
      >
        <Field label="处理人 admin ID" hint="留空则取消分派">
          <input
            class="input"
            value={assignee()}
            onInput={(e) => setAssignee(e.currentTarget.value)}
            placeholder="如 ops@wf.io"
          />
        </Field>
      </Modal>

      {/* 合并弹窗 */}
      <Modal
        open={mergeOpen()}
        onClose={() => setMergeOpen(false)}
        title="合并到目标工单"
        size="sm"
        footer={
          <>
            <Btn variant="ghost" onClick={() => setMergeOpen(false)}>
              取消
            </Btn>
            <Btn variant="danger" disabled={busy() === 'reply' || !mergeTarget().trim()} onClick={doMerge}>
              确认合并
            </Btn>
          </>
        }
      >
        <Field label="目标工单 ID" hint="本工单将被关闭，目标工单去重计数 +1">
          <input
            class="input"
            value={mergeTarget()}
            onInput={(e) => setMergeTarget(e.currentTarget.value)}
            placeholder="目标工单 ID"
          />
        </Field>
      </Modal>
    </Card>
  );
}

/* ---------------------------- 公告 / FAQ 管理 ---------------------------- */
function AnnouncementModal(props: { open: boolean; onClose: () => void }) {
  const [items, { refetch }] = createResource<FeedbackAnnouncement[], boolean>(
    () => (props.open ? true : undefined),
    () => adminApi.feedbackAnnouncements().then((r) => r.data),
  );

  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [title, setTitle] = createSignal('');
  const [body, setBody] = createSignal('');
  const [kind, setKind] = createSignal<AnnouncementKind>('announcement');
  const [published, setPublished] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [delTarget, setDelTarget] = createSignal<FeedbackAnnouncement | null>(null);
  const [deleting, setDeleting] = createSignal(false);

  function resetForm() {
    batch(() => {
      setEditingId(null);
      setTitle('');
      setBody('');
      setKind('announcement');
      setPublished(true);
    });
  }

  function startEdit(a: FeedbackAnnouncement) {
    batch(() => {
      setEditingId(a.id);
      setTitle(a.title);
      setBody(a.body);
      setKind(a.kind);
      setPublished(a.published);
    });
  }

  async function save() {
    const t = title().trim();
    const b = body().trim();
    if (!t) {
      toast.warning('请填写标题');
      return;
    }
    setSaving(true);
    try {
      const id = editingId();
      if (id != null) {
        await adminApi.updateFeedbackAnnouncement(id, {
          title: t,
          body: b,
          kind: kind(),
          published: published(),
        });
        toast.success('已更新');
      } else {
        await adminApi.createFeedbackAnnouncement({
          title: t,
          body: b,
          kind: kind(),
          published: published(),
        });
        toast.success('已发布');
      }
      resetForm();
      refetch();
    } catch (e) {
      toast.error('保存失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setSaving(false);
    }
  }

  async function togglePublish(a: FeedbackAnnouncement) {
    try {
      await adminApi.updateFeedbackAnnouncement(a.id, { published: !a.published });
      refetch();
    } catch (e) {
      toast.error('操作失败', e instanceof Error ? e.message : '未知错误');
    }
  }

  async function confirmDelete() {
    const a = delTarget();
    if (!a) return;
    setDeleting(true);
    try {
      await adminApi.deleteFeedbackAnnouncement(a.id);
      if (editingId() === a.id) resetForm();
      toast.info('已删除');
      setDelTarget(null);
      refetch();
    } catch (e) {
      toast.error('删除失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setDeleting(false);
    }
  }

  function onClose() {
    resetForm();
    props.onClose();
  }

  return (
    <Modal open={props.open} onClose={onClose} title="公告 / FAQ 管理" size="lg">
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 10 })}>
          <Show when={items.state !== 'pending'} fallback={<div style={sx({ display: 'grid', placeItems: 'center', padding: 24 })}><Spinner /></div>}>
            <Show
              when={(items() ?? []).length > 0}
              fallback={<Empty title="暂无公告 / FAQ" desc="使用下方表单创建第一条" icon="msg" />}
            >
              <For each={items()}>
                {(a) => (
                  <div
                    style={sx({
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      gap: 10,
                      padding: 12,
                      borderRadius: 11,
                      border: '1px solid var(--hairline)',
                    })}
                  >
                    <div style={sx({ minWidth: 0 })}>
                      <div style={sx({ display: 'flex', gap: 7, alignItems: 'center', flexWrap: 'wrap' })}>
                        <b style={sx({ fontSize: 13 })}>{a.title}</b>
                        <Badge variant={a.kind === 'faq' ? 'info' : 'accent'}>
                          {a.kind === 'faq' ? 'FAQ' : '公告'}
                        </Badge>
                        <Show when={a.published} fallback={<Badge variant="default">草稿</Badge>}>
                          <Badge variant="success" dot>
                            已发布
                          </Badge>
                        </Show>
                      </div>
                      <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 3 })}>
                        {a.body.slice(0, 60)}
                      </div>
                    </div>
                    <div style={sx({ display: 'flex', gap: 4, alignItems: 'center', flexShrink: 0 })}>
                      <Btn size="xs" variant="ghost" onClick={() => togglePublish(a)}>
                        {a.published ? '下架' : '发布'}
                      </Btn>
                      <Btn size="xs" variant="ghost" icon="edit" onClick={() => startEdit(a)}>
                        编辑
                      </Btn>
                      <IconBtn name="trash" title="删除" onClick={() => setDelTarget(a)} />
                    </div>
                  </div>
                )}
              </For>
            </Show>
          </Show>
        </div>

        <div class="hr" />
        <div class="eyebrow">{editingId() != null ? '编辑条目' : '新建'}</div>
        <div style={sx({ display: 'flex', gap: 10, flexWrap: 'wrap' })}>
          <div style={sx({ flex: 1, minWidth: 200 })}>
            <Field label="标题">
              <input class="input" value={title()} onInput={(e) => setTitle(e.currentTarget.value)} />
            </Field>
          </div>
          <Field label="类型">
            <select
              class="select"
              value={kind()}
              onChange={(e) => setKind(e.currentTarget.value as AnnouncementKind)}
            >
              <option value="announcement">公告</option>
              <option value="faq">FAQ</option>
            </select>
          </Field>
        </div>
        <Field label="正文">
          <textarea
            class="textarea"
            rows={3}
            value={body()}
            onInput={(e) => setBody(e.currentTarget.value)}
          />
        </Field>
        <div style={sx({ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' })}>
          <label style={sx({ display: 'flex', gap: 7, alignItems: 'center', fontSize: 12.5 })}>
            <Switch checked={published()} onChange={setPublished} />
            立即发布
          </label>
          <div style={sx({ display: 'flex', gap: 8, marginLeft: 'auto' })}>
            <Show when={editingId() != null}>
              <Btn variant="ghost" onClick={resetForm}>
                取消编辑
              </Btn>
            </Show>
            <Btn variant="primary" icon={editingId() != null ? 'check' : 'plus'} disabled={saving()} onClick={save}>
              {saving() ? '保存中…' : editingId() != null ? '保存修改' : '发布'}
            </Btn>
          </div>
        </div>
      </div>

      <Confirm
        open={!!delTarget()}
        onClose={() => setDelTarget(null)}
        onConfirm={confirmDelete}
        loading={deleting()}
        danger
        title="删除条目"
        body={`确认删除「${delTarget()?.title ?? ''}」？`}
        confirmText="删除"
      />
    </Modal>
  );
}
