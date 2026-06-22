import { createSignal, createResource, createMemo, createEffect, onMount, onCleanup, Show, For } from 'solid-js';
import {
  Btn, Badge, Card, Field, Tabs, Seg, Empty, Spinner, Loading, Modal, Confirm,
  StatCard, Panel, Progress, PageHead, sx, fmtNum, fmtTime, fmtAgo, fmtPct, toast,
} from '@/components/wf';
import { adminApi } from '@/api/admin';
import type { BroadcastHistoryEntry, BroadcastStats, ScheduledBroadcast } from '@/types/admin';

/**
 * 消息推送（broadcast）。对齐设计稿 pages-messaging.jsx 中 Broadcast 组件：
 * 统一推送中心 —— 统计卡 + 撰写（受众过滤 + 实时受众预估 + 立即/定时 + 草稿存取）
 * + 排程队列（可取消）+ 历史表（可筛选 + 详情）。所有 mock 字段已对齐真实响应。
 */

const PLAT_OPTS = ['web', 'ios', 'android'];

type HistoryFilter = 'all' | 'week' | 'failed';
type ScheduleMode = 'now' | 'at';
type PreviewState = { matched: number; total: number } | 'invalid' | null;

interface Audience {
  platforms?: string[];
  versionMin?: string;
  lastActiveDays?: number;
}

const semverOk = (v: string): boolean => !v.trim() || /^v?\d+\.\d+\.\d+/.test(v.trim());

/** 受众文案：用于确认弹窗 / 预估摘要 / 排程队列回显 */
function audienceLabel(platforms: string[], versionMin: string | null, lastActiveDays: number | null): string {
  const parts: string[] = [];
  if (platforms.length) parts.push(platforms.join('/'));
  if (versionMin) parts.push(`≥${versionMin}`);
  if (lastActiveDays != null) parts.push(`${lastActiveDays}天活跃`);
  return parts.length ? parts.join(' · ') : '全员';
}

function MetricBox(props: { label: string; value: string }) {
  return (
    <div style={sx({ padding: 12, borderRadius: 11, background: 'var(--surface-sunken)', border: '1px solid var(--hairline)', textAlign: 'center' })}>
      <div class="tnum" style={sx({ fontSize: 20, fontWeight: 800 })}>{props.value}</div>
      <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 3 })}>{props.label}</div>
    </div>
  );
}

export default function BroadcastPage() {
  const [tab, setTab] = createSignal<'compose' | 'scheduled' | 'history'>('compose');

  // ---- 撰写表单状态 ----
  const [title, setTitle] = createSignal('');
  const [message, setMessage] = createSignal('');
  const [plats, setPlats] = createSignal<Set<string>>(new Set());
  const [versionMin, setVersionMin] = createSignal('');
  const [lastActiveDays, setLastActiveDays] = createSignal('');
  const [schedule, setSchedule] = createSignal<ScheduleMode>('now');
  const [scheduledAt, setScheduledAt] = createSignal('');
  const [preview, setPreview] = createSignal<PreviewState>(null);
  const [sending, setSending] = createSignal(false);
  const [confirm, setConfirm] = createSignal(false);
  const [savingDraft, setSavingDraft] = createSignal(false);

  // ---- 历史 / 排程 ----
  const [histFilter, setHistFilter] = createSignal<HistoryFilter>('all');
  const [detailBc, setDetailBc] = createSignal<BroadcastHistoryEntry | null>(null);
  const [cancelTarget, setCancelTarget] = createSignal<ScheduledBroadcast | null>(null);

  const [hist, { refetch: refetchHist }] = createResource(
    () => histFilter(),
    (filter) => adminApi.listBroadcasts({ filter, limit: 50 }),
  );
  const [scheduled, { refetch: refetchScheduled }] = createResource(() => adminApi.listScheduledBroadcasts());

  const stats = createMemo<BroadcastStats>(() =>
    hist.error ? { total: 0, totalSent: 0, avgReadRate: 0, online: 0 } : hist()?.stats ?? { total: 0, totalSent: 0, avgReadRate: 0, online: 0 },
  );
  const broadcasts = createMemo<BroadcastHistoryEntry[]>(() => (hist.error ? [] : hist()?.broadcasts ?? []));
  const scheduledItems = createMemo<ScheduledBroadcast[]>(() => (scheduled.error ? [] : scheduled()?.items ?? []));

  // ---- 草稿载入（挂载时一次）----
  onMount(async () => {
    try {
      const { draft } = await adminApi.getPushDraft();
      if (draft) {
        setTitle(draft.title || '');
        setMessage(draft.message || '');
        if (draft.platforms?.length) setPlats(new Set(draft.platforms));
        if (draft.versionMin) setVersionMin(draft.versionMin);
        if (draft.lastActiveDays != null) setLastActiveDays(String(draft.lastActiveDays));
      }
    } catch {
      /* 无草稿或读取失败：静默 */
    }
  });

  const buildAudience = (): Audience | undefined => {
    const p = [...plats()];
    const v = versionMin().trim();
    const d = lastActiveDays().trim();
    if (!p.length && !v && !d) return undefined;
    return {
      platforms: p.length ? p : undefined,
      versionMin: v || undefined,
      lastActiveDays: d ? Number(d) : undefined,
    };
  };

  // ---- 受众预估（防抖 450ms，受众条件任一变化重取）----
  createEffect(() => {
    const _p = plats();
    const v = versionMin();
    const d = lastActiveDays();
    void _p; void d;
    const handle = setTimeout(async () => {
      if (v.trim() && !semverOk(v)) {
        setPreview('invalid');
        return;
      }
      setPreview(null);
      try {
        const r = await adminApi.broadcastPreview({ audience: buildAudience() });
        setPreview(r);
      } catch {
        setPreview({ matched: 0, total: 0 });
      }
    }, 450);
    onCleanup(() => clearTimeout(handle));
  });

  function togglePlat(p: string) {
    setPlats((s) => {
      const n = new Set(s);
      if (n.has(p)) n.delete(p);
      else n.add(p);
      return n;
    });
  }

  const canSend = createMemo(() => !!title().trim() && !!message().trim() && (schedule() !== 'at' || !!scheduledAt()));

  const audienceText = createMemo(() => {
    const a = buildAudience();
    if (!a) return '全员';
    const parts: string[] = [];
    if (a.platforms) parts.push(a.platforms.join('/'));
    if (a.versionMin) parts.push(`≥${a.versionMin}`);
    if (a.lastActiveDays != null) parts.push(`${a.lastActiveDays}天活跃`);
    return parts.length ? parts.join(' · ') : '全员';
  });

  async function doSend() {
    setConfirm(false);
    setSending(true);
    try {
      const at = schedule() === 'at' && scheduledAt() ? new Date(scheduledAt()).toISOString() : undefined;
      const r = await adminApi.broadcast({ title: title(), message: message(), audience: buildAudience(), scheduledAt: at });
      if (r.scheduled) {
        toast.success('已排程', `将于 ${fmtTime(r.scheduledAt ?? '')} 自动下发`);
        await refetchScheduled();
      } else {
        toast.success(`已发送给 ${fmtNum(r.sent ?? 0)} 位用户`);
      }
      // 发送成功后清空已用过的草稿，复位表单
      try {
        await adminApi.deletePushDraft();
      } catch {
        /* 无草稿可删：忽略 */
      }
      setTitle('');
      setMessage('');
      setSchedule('now');
      setScheduledAt('');
      await refetchHist();
    } catch (err: unknown) {
      toast.error('发送失败', err instanceof Error ? err.message : '');
    } finally {
      setSending(false);
    }
  }

  async function saveDraft() {
    if (!title().trim() && !message().trim()) {
      toast.warning('草稿至少需要标题或正文');
      return;
    }
    setSavingDraft(true);
    try {
      const a = buildAudience();
      await adminApi.savePushDraft({
        title: title(),
        message: message(),
        platforms: a?.platforms,
        versionMin: a?.versionMin ?? null,
        lastActiveDays: a?.lastActiveDays ?? null,
      });
      toast.success('草稿已保存');
    } catch (err: unknown) {
      toast.error('保存失败', err instanceof Error ? err.message : '');
    } finally {
      setSavingDraft(false);
    }
  }

  async function doCancelScheduled() {
    const target = cancelTarget();
    if (!target) return;
    try {
      await adminApi.cancelScheduledBroadcast(target.id);
      toast.success('已取消该排程');
      await refetchScheduled();
    } catch (err: unknown) {
      toast.error('取消失败', err instanceof Error ? err.message : '');
    } finally {
      setCancelTarget(null);
    }
  }

  const previewMatched = createMemo(() => {
    const p = preview();
    return p && p !== 'invalid' ? p.matched : 0;
  });
  // 有效预估对象（剥离 'invalid' / null），供受众预估卡的 <Show> 回调取到精确类型
  const previewData = createMemo<{ matched: number; total: number } | null>(() => {
    const p = preview();
    return p && p !== 'invalid' ? p : null;
  });

  return (
    <div>
      <PageHead
        title="消息推送"
        desc="统一的推送中心：撰写广播、按受众精准投递、定时排程与历史回看。强制升级推送在「设备与遥测」内发起。"
      />

      {/* ====== 统计卡 ====== */}
      <Show when={hist()}>
        <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit,minmax(180px,1fr))', gap: 14, marginBottom: 16 })}>
          <StatCard tone="accent" label="广播总数" icon="send" value={fmtNum(stats().total)} />
          <StatCard tone="info" label="累计触达" icon="users" value={fmtNum(stats().totalSent)} />
          <StatCard tone="success" label="平均阅读率" icon="check" value={(stats().avgReadRate * 100).toFixed(0)} unit="%" />
          <StatCard tone="warning" label="待发排程" icon="clock" value={fmtNum(scheduledItems().length)} />
        </div>
      </Show>

      <Tabs
        value={tab()}
        onChange={(v) => setTab(v as 'compose' | 'scheduled' | 'history')}
        tabs={[
          { value: 'compose', label: '撰写' },
          { value: 'scheduled', label: '排程队列', count: scheduledItems().length },
          { value: 'history', label: '历史' },
        ]}
      />

      <div style={sx({ marginTop: 16 })}>
        {/* ====== 撰写 ====== */}
        <Show when={tab() === 'compose'}>
          <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.3fr) minmax(0,1fr)', gap: 16 })}>
            <Panel title="撰写广播">
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 13 })}>
                <Field label="标题">
                  <input
                    class="input"
                    value={title()}
                    disabled={sending()}
                    onInput={(e) => setTitle(e.currentTarget.value)}
                    placeholder="如：v1.4.3 更新公告"
                  />
                </Field>
                <Field label="正文">
                  <textarea
                    class="textarea"
                    rows={5}
                    value={message()}
                    disabled={sending()}
                    onInput={(e) => setMessage(e.currentTarget.value)}
                    placeholder="支持纯文本…"
                  />
                </Field>

                <div class="hr" />
                <div class="eyebrow">受众过滤</div>

                <Field label="平台">
                  <div style={sx({ display: 'flex', gap: 8 })}>
                    <For each={PLAT_OPTS}>
                      {(p) => {
                        const on = createMemo(() => plats().has(p));
                        return (
                          <button
                            type="button"
                            class="badge"
                            onClick={() => togglePlat(p)}
                            style={sx({
                              cursor: 'pointer',
                              padding: '6px 13px',
                              border: `1px solid ${on() ? 'var(--accent)' : 'var(--border)'}`,
                              background: on() ? 'var(--accent-soft)' : 'var(--surface-sunken)',
                              color: on() ? 'var(--accent)' : 'var(--text-2)',
                            })}
                          >
                            {p}
                          </button>
                        );
                      }}
                    </For>
                  </div>
                </Field>

                <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 })}>
                  <Field label="最低版本" hint={versionMin().trim() && !semverOk(versionMin()) ? '⚠ 非法版本号' : ''}>
                    <input
                      class="input"
                      value={versionMin()}
                      onInput={(e) => setVersionMin(e.currentTarget.value)}
                      placeholder="1.3.0"
                      style={sx({ borderColor: versionMin().trim() && !semverOk(versionMin()) ? 'var(--error)' : undefined })}
                    />
                  </Field>
                  <Field label="活跃天数内">
                    <input
                      class="input"
                      type="number"
                      value={lastActiveDays()}
                      onInput={(e) => setLastActiveDays(e.currentTarget.value)}
                      placeholder="7"
                    />
                  </Field>
                </div>

                <div class="hr" />

                <Field label="投递时机">
                  <div style={sx({ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' })}>
                    <Seg
                      options={[{ value: 'now', label: '立即' }, { value: 'at', label: '定时' }]}
                      value={schedule()}
                      onChange={(v) => setSchedule(v as ScheduleMode)}
                    />
                    <Show when={schedule() === 'at'}>
                      <input
                        class="input"
                        type="datetime-local"
                        style={sx({ width: 'auto' })}
                        value={scheduledAt()}
                        onInput={(e) => setScheduledAt(e.currentTarget.value)}
                      />
                    </Show>
                  </div>
                </Field>

                <div style={sx({ display: 'flex', gap: 8, justifyContent: 'flex-end' })}>
                  <Btn variant="ghost" icon="copy" onClick={saveDraft} disabled={savingDraft()}>
                    {savingDraft() ? '保存中…' : '存草稿'}
                  </Btn>
                  <Btn variant="primary" icon="send" disabled={!canSend() || sending()} onClick={() => setConfirm(true)}>
                    {schedule() === 'at' ? '排程发送' : '立即发送'}
                  </Btn>
                </div>
              </div>
            </Panel>

            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 16 })}>
              <Panel title="受众预估">
                <div style={sx({ display: 'flex', flexDirection: 'column', alignItems: 'center', padding: '10px 0' })}>
                  <Show
                    when={preview() !== 'invalid'}
                    fallback={<span style={sx({ color: 'var(--error)', fontSize: 13 })}>版本号非法</span>}
                  >
                    <Show when={previewData()} fallback={<Spinner />}>
                      {(p) => (
                        <>
                          <div class="tnum" style={sx({ fontSize: 38, fontWeight: 800, color: 'var(--accent)' })}>
                            {fmtNum(p().matched)}
                          </div>
                          <div class="muted" style={sx({ fontSize: 12.5 })}>
                            / {fmtNum(p().total)} 用户 · {audienceText()}
                          </div>
                          <div style={sx({ width: '100%', marginTop: 12 })}>
                            <Progress value={p().total > 0 ? (p().matched / p().total) * 100 : 0} tone="accent" height={8} />
                          </div>
                        </>
                      )}
                    </Show>
                  </Show>
                </div>
              </Panel>

              <Panel title="预览">
                <div style={sx({ padding: 14, borderRadius: 12, background: 'var(--surface-sunken)', border: '1px solid var(--hairline)' })}>
                  <div style={sx({ display: 'flex', gap: 9, alignItems: 'center', marginBottom: 8 })}>
                    <span style={sx({ width: 28, height: 28, borderRadius: '50%', background: 'var(--accent)', color: 'var(--solid-ink)', display: 'grid', placeItems: 'center', fontWeight: 700, fontSize: 13 })}>W</span>
                    <b style={sx({ fontSize: 13 })}>WordForge</b>
                  </div>
                  <div style={sx({ fontWeight: 700, fontSize: 14 })}>{title() || '广播标题'}</div>
                  <div class="muted" style={sx({ fontSize: 12.5, marginTop: 5, lineHeight: 1.55, whiteSpace: 'pre-wrap' })}>
                    {message() || '广播正文将显示在这里…'}
                  </div>
                </div>
              </Panel>
            </div>
          </div>
        </Show>

        {/* ====== 排程队列 ====== */}
        <Show when={tab() === 'scheduled'}>
          <Card pad={false} style={sx({ overflow: 'hidden' })}>
            <Show when={scheduled.state !== 'pending'} fallback={<Loading />}>
              <Show
                when={scheduledItems().length > 0}
                fallback={<Empty title="暂无待发排程" desc="在「撰写」中选择定时发送即可排程。" icon="clock" />}
              >
                <table class="tbl">
                  <thead>
                    <tr>
                      <th>标题</th>
                      <th>受众</th>
                      <th>投递时间</th>
                      <th style={sx({ textAlign: 'right' })}>操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={scheduledItems()}>
                      {(s) => (
                        <tr>
                          <td style={sx({ fontWeight: 600 })}>{s.title}</td>
                          <td class="muted" style={sx({ fontSize: 12 })}>{audienceLabel(s.platforms, s.versionMin, s.lastActiveDays)}</td>
                          <td class="mono" style={sx({ fontSize: 12 })}>{fmtTime(s.scheduledAt)}</td>
                          <td style={sx({ textAlign: 'right' })}>
                            <Btn size="xs" variant="danger" onClick={() => setCancelTarget(s)}>取消</Btn>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </Show>
            </Show>
          </Card>
        </Show>

        {/* ====== 历史 ====== */}
        <Show when={tab() === 'history'}>
          <Card pad={false} style={sx({ overflow: 'hidden' })}>
            <div style={sx({ padding: 12, borderBottom: '1px solid var(--hairline)' })}>
              <Seg
                options={[{ value: 'all', label: '全部' }, { value: 'week', label: '本周' }, { value: 'failed', label: '失败' }]}
                value={histFilter()}
                onChange={(v) => setHistFilter(v as HistoryFilter)}
              />
            </div>
            <Show when={hist.state !== 'pending'} fallback={<Loading />}>
              <Show when={!hist.error} fallback={<Empty title="加载失败" desc="请稍后重试。" icon="alert" />}>
                <Show when={broadcasts().length > 0} fallback={<Empty title="暂无广播记录" icon="send" />}>
                  <table class="tbl">
                    <thead>
                      <tr>
                        <th>标题</th>
                        <th>送达</th>
                        <th>阅读率</th>
                        <th>发送人</th>
                        <th>时间</th>
                        <th />
                      </tr>
                    </thead>
                    <tbody>
                      <For each={broadcasts()}>
                        {(b) => (
                          <tr>
                            <td style={sx({ fontWeight: 600 })}>{b.title}</td>
                            <td class="mono">{fmtNum(b.sentCount)}</td>
                            <td class="mono">{(b.readRate * 100).toFixed(0)}%</td>
                            <td class="muted" style={sx({ fontSize: 12 })}>{b.author}</td>
                            <td class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtAgo(b.createdAt)}</td>
                            <td>
                              <Btn size="xs" variant="ghost" onClick={() => setDetailBc(b)}>详情</Btn>
                            </td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </Show>
              </Show>
            </Show>
          </Card>
        </Show>
      </div>

      {/* ====== 历史详情 ====== */}
      <Modal open={!!detailBc()} onClose={() => setDetailBc(null)} title="广播详情" size="md">
        <Show when={detailBc()}>
          {(d) => (
            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 12 })}>
              <div style={sx({ fontWeight: 700, fontSize: 16 })}>{d().title}</div>
              <div class="muted" style={sx({ fontSize: 13, lineHeight: 1.6, whiteSpace: 'pre-wrap' })}>{d().message}</div>
              <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: 10 })}>
                <MetricBox label="送达" value={fmtNum(d().sentCount)} />
                <MetricBox label="阅读率" value={`${(d().readRate * 100).toFixed(0)}%`} />
                <MetricBox label="已读" value={fmtNum(d().readCount)} />
              </div>
              <div class="muted" style={sx({ fontSize: 12.5 })}>发送人 {d().author} · {fmtTime(d().createdAt)}</div>
            </div>
          )}
        </Show>
      </Modal>

      {/* ====== 发送确认 ====== */}
      <Confirm
        open={confirm()}
        onClose={() => setConfirm(false)}
        onConfirm={doSend}
        loading={sending()}
        title={schedule() === 'at' ? '确认排程发送？' : '确认发送广播？'}
        confirmText="确认发送"
        body={
          <>
            本次将{schedule() === 'at' ? '于指定时间' : '立即'}推送给「{audienceText()}」
            <Show when={preview() && preview() !== 'invalid'}>（约 {fmtNum(previewMatched())} 人）</Show>。
          </>
        }
      />

      {/* ====== 取消排程确认 ====== */}
      <Confirm
        open={!!cancelTarget()}
        onClose={() => setCancelTarget(null)}
        onConfirm={doCancelScheduled}
        danger
        title="取消排程"
        confirmText="取消排程"
        body={<>确认取消「{cancelTarget()?.title}」的定时发送？</>}
      />
    </div>
  );
}
