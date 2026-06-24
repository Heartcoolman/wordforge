import { createSignal, createMemo, createResource, createEffect, onCleanup, startTransition, For, Show, type JSX } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import {
  Btn, Badge, Card, Panel, Field, Switch, Tabs, Empty, Skel, Loading, Modal, Drawer,
  Progress, Pagination, PageHead, Icon, fmtNum, fmtDur, fmtAgo, sx, toast,
} from '@/components/wf';
import {
  adminApi,
  type SseLiveEntry, type RecentlyActiveEntry, type DataChannelStatus, type DataChannelValue,
  type TelemetrySummary, type FlaggedClientEntry,
} from '@/api/admin';
import type {
  ClientPlatformAgg, ClientVersionAgg, ClientUpgradePolicy, ListedDevice, VersionGate,
} from '@/types/admin';

/* ============================================================
   设备与遥测 — 平台聚合 · SSE 实时 · 近期活跃 · 全部设备
   升级策略 · 版本门控 · 强制升级广播 · 单设备遥测抽屉
   ============================================================ */

const CHAN_LABEL: Record<string, string> = { amas: 'AMAS', learning: '学习', telemetry: '遥测' };
const CHAN_VARIANT: Record<DataChannelValue, 'success' | 'warning' | 'error'> = {
  uploaded: 'success', nil: 'warning', none: 'error',
};

type PlatKey = 'web' | 'ios' | 'android';
const PLAT_META: Record<PlatKey, { name: string; sub: string; icon: string; col: string }> = {
  web: { name: 'Web 浏览器', sub: 'PWA · Chrome / Safari', icon: 'remote', col: 'var(--accent)' },
  ios: { name: 'iOS 原生', sub: 'iPhone / iPad', icon: 'devices', col: 'var(--info)' },
  android: { name: 'Android 原生', sub: 'Google Play / APK', icon: 'cpu', col: 'var(--success)' },
};
const PLATS: PlatKey[] = ['web', 'ios', 'android'];

const shortId = (s: string | null | undefined): string =>
  !s ? '—' : s.length > 14 ? s.slice(0, 9) + '…' + s.slice(-4) : s;

// 事件 type → 系统分类(与探针看板口径一致)
function catOf(type: string): { key: string; label: string; col: string } {
  const t = (type || '').toLowerCase();
  if (t.includes('error') || t.includes('err')) return { key: 'err', label: '错误', col: 'var(--error)' };
  if (/(lesson|word|answer|session|review)/.test(t)) return { key: 'learn', label: '学习', col: 'var(--success)' };
  if (/(perf|resource|nav)/.test(t)) return { key: 'perf', label: '性能', col: 'var(--info)' };
  return { key: 'behavior', label: '行为', col: 'var(--accent)' };
}

function ChanBadges(props: { ch: DataChannelStatus }) {
  return (
    <div style={sx({ display: 'flex', gap: 4 })}>
      <For each={['amas', 'learning', 'telemetry'] as const}>
        {(k) => (
          <Badge variant={CHAN_VARIANT[props.ch[k]]} dot>
            {CHAN_LABEL[k]}
          </Badge>
        )}
      </For>
    </div>
  );
}

function MetricBox(props: { label: string; value: JSX.Element | string | number }) {
  return (
    <div style={sx({ padding: '9px 11px', borderRadius: 10, background: 'var(--surface-sunken)' })}>
      <div class="muted-3" style={sx({ fontSize: 10.5, marginBottom: 3 })}>{props.label}</div>
      <div style={sx({ fontSize: 13.5, fontWeight: 600 })}>{props.value}</div>
    </div>
  );
}

function NameCounts(props: { title: string; items: { name: string; count: number }[] }) {
  const max = () => Math.max(1, ...props.items.map((i) => i.count));
  return (
    <div>
      <div class="muted-3" style={sx({ fontSize: 11, marginBottom: 6 })}>{props.title}</div>
      <Show when={props.items.length} fallback={<div class="muted-3" style={sx({ fontSize: 11.5 })}>暂无</div>}>
        <For each={props.items.slice(0, 4)}>
          {(it) => (
            <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 36px', gap: 6, alignItems: 'center', fontSize: 11.5, marginBottom: 4 })}>
              <div>
                <div style={sx({ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{it.name}</div>
                <div class="bar" style={sx({ height: 5, marginTop: 2 })}><i style={sx({ width: (it.count / max() * 100) + '%' })} /></div>
              </div>
              <span class="mono muted" style={sx({ textAlign: 'right' })}>{it.count}</span>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

/* ============================================================
   升级策略编辑器(每平台一行)
   ============================================================ */
function PolicyEditor(props: {
  policy: ClientUpgradePolicy;
  versions: ClientVersionAgg[];
  onSaved: () => void;
  onForce: (below: string, latest: string) => void;
}) {
  const [draft, setDraft] = createSignal<ClientUpgradePolicy>(props.policy);
  const [busy, setBusy] = createSignal(false);
  // 切换 policy 引用时重置草稿
  createEffect(() => { void props.policy.platform; setDraft({ ...props.policy }); });

  const dirty = () => JSON.stringify(draft()) !== JSON.stringify(props.policy);
  const meta = () => PLAT_META[props.policy.platform as PlatKey] ?? PLAT_META.web;
  const totalV = () => props.versions.reduce((a, v) => a + v.count, 0) || 1;

  const save = async () => {
    setBusy(true);
    try {
      const d = draft();
      await adminApi.putUpgradePolicy(props.policy.platform, {
        minVersion: d.minVersion,
        suggestedVersion: d.suggestedVersion,
        grayscalePct: d.grayscalePct,
        pwaSilentUpdate: d.pwaSilentUpdate,
      });
      toast.success(`${props.policy.platform} 升级策略已保存`);
      props.onSaved();
    } catch (e: any) {
      toast.error(`${props.policy.platform} 策略保存失败`, e?.message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={sx({ border: '1px solid var(--hairline)', borderRadius: 12, padding: 14 })}>
      <div style={sx({ display: 'flex', alignItems: 'center', gap: 9, marginBottom: 10 })}>
        <span style={sx({ width: 26, height: 26, borderRadius: 8, display: 'grid', placeItems: 'center', background: `color-mix(in oklch, ${meta().col} 15%, transparent)`, color: meta().col })}>
          <Icon name={meta().icon} size={14} />
        </span>
        <b style={sx({ fontSize: 13.5 })}>{meta().name}</b>
        <span class="muted-3" style={sx({ marginLeft: 'auto', fontSize: 11.5 })}>{props.versions.length} 个在用版本</span>
      </div>

      {/* 版本柱 */}
      <Show when={props.versions.length} fallback={<div class="muted-3" style={sx({ fontSize: 11.5, marginBottom: 12 })}>— 暂无版本上报</div>}>
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 5, marginBottom: 12 })}>
          <For each={props.versions.slice(0, 4)}>
            {(v) => (
              <div style={sx({ display: 'grid', gridTemplateColumns: '64px 1fr 44px', gap: 8, alignItems: 'center', fontSize: 11.5 })}>
                <span class="mono" style={sx({ overflow: 'hidden', textOverflow: 'ellipsis' })}>{v.version === 'unknown' ? '未上报' : v.version}</span>
                <div class="bar" style={sx({ height: 7 })}><i style={sx({ width: (v.count / totalV() * 100) + '%', background: meta().col })} /></div>
                <span class="mono muted" style={sx({ textAlign: 'right' })}>{v.count}</span>
              </div>
            )}
          </For>
        </div>
      </Show>

      <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 9 })}>
        <Field label="最低版本">
          <input class="input" value={draft().minVersion ?? ''} placeholder="未设置"
            onInput={(e) => setDraft({ ...draft(), minVersion: e.currentTarget.value || null })} />
        </Field>
        <Field label="建议版本">
          <input class="input" value={draft().suggestedVersion ?? ''} placeholder="未设置"
            onInput={(e) => setDraft({ ...draft(), suggestedVersion: e.currentTarget.value || null })} />
        </Field>
      </div>

      <div style={sx({ display: 'flex', gap: 8, marginTop: 12, justifyContent: 'flex-end' })}>
        <Show when={props.policy.minVersion}>
          <Btn size="sm" variant="warning" icon="zap"
            onClick={() => props.onForce(props.policy.minVersion!, props.policy.suggestedVersion || props.policy.minVersion!)}>
            强制升级低版本
          </Btn>
        </Show>
        <Btn size="sm" variant="primary" disabled={!dirty() || busy()} onClick={save}>
          {busy() ? '保存中…' : '保存策略'}
        </Btn>
      </div>
    </div>
  );
}

/* ============================================================
   全局版本门控编辑器
   ============================================================ */
const VG_SEMVER_RE = /^\d+\.\d+\.\d+(?:[-+].+)?$/;

function VersionGateEditor(props: { gate: VersionGate; onSaved: () => void }) {
  const [enabled, setEnabled] = createSignal(props.gate.enabled);
  const [min, setMin] = createSignal(props.gate.minClientVersion ?? '');
  const [busy, setBusy] = createSignal(false);
  createEffect(() => { setEnabled(props.gate.enabled); setMin(props.gate.minClientVersion ?? ''); });

  const invalid = () => min().trim().length > 0 && !VG_SEMVER_RE.test(min().trim());
  const dirty = () => enabled() !== props.gate.enabled || min().trim() !== (props.gate.minClientVersion ?? '');

  const save = async () => {
    if (invalid()) { toast.error('版本号非法', '需为合法 semver,如 1.2.0'); return; }
    setBusy(true);
    try {
      await adminApi.setVersionGate({ enabled: enabled(), minClientVersion: min().trim() || null });
      toast.success('版本门控已更新', enabled() ? `低于 ${min()} 的客户端将被拒绝` : '门控已关闭');
      props.onSaved();
    } catch (e: any) {
      toast.error('版本门控保存失败', e?.message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
      <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '12px 14px', borderRadius: 11, background: 'var(--surface-sunken)' })}>
        <div>
          <div style={sx({ fontWeight: 600, fontSize: 13 })}>启用版本门控</div>
          <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 2 })}>低于阈值的客户端无法连接</div>
        </div>
        <Switch checked={enabled()} onChange={setEnabled} />
      </div>
      <Field label="最低客户端版本" hint={invalid() ? '⚠ 非法 semver' : '生效后立即拦截低版本握手'}>
        <input class="input" value={min()} placeholder="1.2.0"
          onInput={(e) => setMin(e.currentTarget.value)}
          style={sx({ borderColor: invalid() ? 'var(--error)' : undefined })} />
      </Field>
      <div style={sx({ padding: '10px 12px', borderRadius: 10, background: enabled() ? 'var(--warning-soft)' : 'var(--surface-sunken)', fontSize: 12, color: enabled() ? 'var(--warning)' : 'var(--text-3)' })}>
        {enabled()
          ? `当前门控开启:客户端版本 < ${min() || '—'} 将被拒绝握手。`
          : '门控关闭:所有版本可正常连接。'}
      </div>
      <Show when={props.gate.envMinClientVersion}>
        <div class="muted-3" style={sx({ fontSize: 11.5 })}>
          环境兜底版本 <span class="mono">{props.gate.envMinClientVersion}</span> · 实际生效 <span class="mono">{props.gate.effectiveMinClientVersion ?? '—'}</span>
        </div>
      </Show>
      <Btn variant="primary" disabled={!dirty() || busy()} onClick={save}>{busy() ? '保存中…' : '保存门控'}</Btn>
    </div>
  );
}

/* ============================================================
   强制升级广播 modal
   ============================================================ */
function ForceUpgradeModal(props: {
  target: { platform: string; below: string; latest: string } | null;
  onClose: () => void;
}) {
  const [msg, setMsg] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  createEffect(() => { if (props.target) setMsg(''); });

  const send = async () => {
    const t = props.target;
    if (!t) return;
    setBusy(true);
    try {
      const r = await adminApi.broadcastUpgrade(t.platform, {
        belowVersion: t.below, latestVersion: t.latest, message: msg() || undefined,
      });
      toast.success(`已派发 ${t.platform} 强制升级`, `匹配 ${r.matched} 设备 · 触达 ${r.pushedConnections} 个连接`);
      props.onClose();
    } catch (e: any) {
      toast.error('强制升级广播失败', e?.message);
    } finally {
      setBusy(false);
    }
  };

  const revoke = async () => {
    const t = props.target;
    if (!t) return;
    setBusy(true);
    try {
      const r = await adminApi.revokeUpgrade(t.platform);
      toast.success(`已撤销 ${t.platform} 强制升级`, `触达 ${r.pushedConnections} 个连接 · 覆盖 ${r.devices} 设备`);
      props.onClose();
    } catch (e: any) {
      toast.error('撤销强制升级失败', e?.message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={!!props.target} onClose={props.onClose} title="强制升级广播" size="sm"
      footer={
        <>
          <Btn variant="ghost" onClick={props.onClose}>取消</Btn>
          <Btn variant="danger" icon="rotate" onClick={revoke} disabled={busy()}>{busy() ? '处理中…' : '撤销强升'}</Btn>
          <Btn variant="warning" icon="zap" onClick={send} disabled={busy()}>{busy() ? '派发中…' : '派发升级'}</Btn>
        </>
      }
    >
      <Show when={props.target}>
        {(t) => (
          <>
            <p class="muted" style={sx({ fontSize: 13, marginTop: 0 })}>
              向 <b>{t().platform}</b> 平台上低于 <span class="mono">{t().below}</span> 的设备推送强制升级到 <span class="mono">{t().latest}</span>。
            </p>
            <Field label="附加提示(可选)">
              <textarea class="textarea" rows={3} value={msg()} onInput={(e) => setMsg(e.currentTarget.value)}
                placeholder="如:本次升级修复重要安全问题,请尽快更新。" />
            </Field>
          </>
        )}
      </Show>
    </Modal>
  );
}

/* ============================================================
   单设备遥测详情抽屉
   ============================================================ */
function DeviceDrawer(props: {
  deviceId: string | null;
  onClose: () => void;
  onBan: (id: string, action: 'ban' | 'unban') => void;
  onClearFlag: (id: string) => void;
}) {
  const [filter, setFilter] = createSignal('');
  const [expanded, setExpanded] = createSignal<string | null>(null);
  createEffect(() => { void props.deviceId; setFilter(''); setExpanded(null); });

  const [detail] = createResource(() => props.deviceId, (id) => adminApi.getClientDetail(id));
  const [summary] = createResource(() => props.deviceId, (id) => adminApi.getTelemetrySummary(id));
  const [recs] = createResource(
    () => (props.deviceId ? { id: props.deviceId, f: filter() } : null),
    (dep) => adminApi.getTelemetry(dep.id, dep.f ? { eventType: dep.f } : undefined),
  );

  const d = () => detail();
  const s = () => summary();

  return (
    <Drawer
      open={!!props.deviceId} onClose={props.onClose} title="设备遥测详情" subtitle={shortId(props.deviceId)} width={560}
      footer={
        <Show when={d()}>
          {(dt) => (
            <Btn variant={dt().isBanned ? 'success' : 'danger'} icon="ban"
              onClick={() => { props.onBan(props.deviceId!, dt().isBanned ? 'unban' : 'ban'); props.onClose(); }}>
              {dt().isBanned ? '解封' : '封禁'}
            </Btn>
          )}
        </Show>
      }
    >
      <Show when={!detail.loading && d()} fallback={<Loading />}>
        {(dt) => (
          <>
            <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 9, marginBottom: 16 })}>
              <MetricBox label="平台" value={dt().platform} />
              <MetricBox label="版本" value={dt().appVersion ?? '—'} />
              <MetricBox label="状态" value={dt().online ? '在线' : '离线'} />
              <MetricBox label="国家" value={dt().country ?? '—'} />
              <MetricBox label="机型" value={dt().model ?? '—'} />
              <MetricBox label="连接数" value={dt().connectionCount} />
            </div>

            <Show when={dt().isBanned}>
              <div style={sx({ padding: '9px 12px', borderRadius: 10, background: 'var(--error-soft)', color: 'var(--error)', fontSize: 12.5, marginBottom: 14 })}>
                已封禁 · {dt().banReason ?? '无原因'} · {fmtAgo(dt().bannedAt)}
              </div>
            </Show>

            {/* m054:关联风控标记(共享 IP/账号/指纹被牵连),供复核 + 一键解除 */}
            <Show when={dt().riskFlag}>
              <div style={sx({ display: 'flex', alignItems: 'center', gap: 8, padding: '9px 12px', borderRadius: 10, background: 'var(--warning-soft)', color: 'var(--warning)', fontSize: 12.5, marginBottom: 14 })}>
                <Icon name="zap" size={14} />
                <div style={sx({ flex: 1 })}>
                  关联风险标记 · {dt().riskReason ?? '无原因'} · {fmtAgo(dt().riskFlaggedAt)}
                </div>
                <Btn size="xs" variant="outline" onClick={() => { props.onClearFlag(props.deviceId!); props.onClose(); }}>解除</Btn>
              </div>
            </Show>

            {/* 遥测总览 */}
            <Show when={!summary.loading && s()} fallback={<Loading h={120} />}>
              {(sm) => (
                <>
                  <div class="eyebrow" style={sx({ marginBottom: 8 })}>
                    遥测总览 · {sm().total} 条 · {sm().sessionCount} 会话
                  </div>
                  <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 9, marginBottom: 14 })}>
                    <MetricBox label="总点击" value={fmtNum(sm().totalClicks)} />
                    <MetricBox label="总错误" value={sm().totalErrors} />
                    <MetricBox label="总时长" value={fmtDur(sm().totalDurationSecs)} />
                    <MetricBox label="事件类型" value={sm().byEventType.length} />
                  </div>
                  <div style={sx({ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 14 })}>
                    <button class="badge" onClick={() => setFilter('')}
                      style={sx({ cursor: 'pointer', border: '1px solid ' + (!filter() ? 'var(--accent)' : 'var(--border)'), background: !filter() ? 'var(--accent-soft)' : 'var(--surface-sunken)', color: !filter() ? 'var(--accent)' : 'var(--text-2)', padding: '4px 10px' })}>
                      全部 <span class="mono">{sm().total}</span>
                    </button>
                    <For each={sm().byEventType}>
                      {(et) => {
                        const c = catOf(et.eventType);
                        return (
                          <button class="badge" onClick={() => setFilter(et.eventType)}
                            style={sx({ cursor: 'pointer', border: '1px solid ' + (filter() === et.eventType ? c.col : 'var(--border)'), background: 'var(--surface-sunken)', padding: '4px 10px' })}>
                            <span style={sx({ width: 6, height: 6, borderRadius: 50, background: c.col, display: 'inline-block', marginRight: 4 })} />
                            {et.eventType} <span class="mono">{et.count}</span>
                          </button>
                        );
                      }}
                    </For>
                  </div>
                  <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 14, marginBottom: 14 })}>
                    <NameCounts title="高频功能" items={sm().featureUsage} />
                    <NameCounts title="访问页面" items={sm().routes} />
                  </div>
                </>
              )}
            </Show>

            {/* 遥测记录 */}
            <div class="eyebrow" style={sx({ marginBottom: 8 })}>遥测记录{filter() ? ' · ' + filter() : ''}</div>
            <Show when={recs.state !== 'pending'} fallback={<Loading h={120} />}>
              <Show when={(recs()?.records ?? []).length} fallback={<Empty title="暂无遥测记录" desc="该设备未上报匹配的遥测明细" icon="inbox" />}>
                <For each={(recs()?.records ?? []).slice(0, 20)}>
                  {(r: TelemetrySummary) => {
                    const c = catOf(r.eventType);
                    const open = () => expanded() === r.id;
                    return (
                      <div style={sx({ borderBottom: '1px solid var(--hairline)' })}>
                        <button onClick={() => setExpanded(open() ? null : r.id)}
                          style={sx({ width: '100%', display: 'flex', alignItems: 'center', gap: 9, padding: '10px 0', background: 'transparent', border: 'none', cursor: 'pointer', textAlign: 'left' })}>
                          <span style={sx({ width: 7, height: 7, borderRadius: 50, background: c.col, flex: 'none' })} />
                          <span class="mono" style={sx({ fontSize: 12, fontWeight: 600, flex: 1 })}>{r.eventType}</span>
                          <Badge variant="default">{c.label}</Badge>
                          <span class="muted-3" style={sx({ fontSize: 11 })}>{fmtAgo(r.serverTs)}</span>
                          <Icon name="chevD" size={14} style={sx({ color: 'var(--text-3)', transform: open() ? 'rotate(180deg)' : 'none', transition: 'transform .2s' })} />
                        </button>
                        <Show when={open()}>
                          <div class="fade-in" style={sx({ padding: '4px 0 12px 16px', fontSize: 11.5 })}>
                            <div class="muted" style={sx({ lineHeight: 1.9 })}>
                              路由 <b class="mono">{r.behaviorSummary.currentRoute ?? '—'}</b> · 点击 {r.behaviorSummary.clickCount ?? 0} · 时长 {fmtDur(r.sessionStats.sessionDurationSecs)} · 错误 {r.sessionStats.errorCount}
                              <br />
                              {r.deviceProfile.osName ?? '—'} · {r.deviceProfile.browserName ?? '—'} · {r.deviceProfile.screenWidth ?? '—'}×{r.deviceProfile.screenHeight ?? '—'}
                            </div>
                          </div>
                        </Show>
                      </div>
                    );
                  }}
                </For>
              </Show>
            </Show>
          </>
        )}
      </Show>
    </Drawer>
  );
}

/* ============================================================
   主页面
   ============================================================ */
// 表格行统一形状(SSE / Recent / All 三类来源映射后渲染)
type DeviceRow = {
  deviceId: string; platform: string; appVersion?: string | null; userId: string | null;
  isBanned: boolean; dataChannels?: DataChannelStatus;
  connectedSecs?: number; connectionCount?: number; lastSeenAt?: string; country?: string | null;
  riskFlag?: boolean; riskRelatedDevice?: string | null;
};

export default function DevicesPage() {
  const navigate = useNavigate();
  const [tab, setTab] = createSignal<'sse' | 'recent' | 'all' | 'flagged'>('sse');
  const [detail, setDetail] = createSignal<string | null>(null);
  const [banTarget, setBanTarget] = createSignal<{ id: string; action: 'ban' | 'unban' } | null>(null);
  const [banReason, setBanReason] = createSignal('');
  const [upgradeModal, setUpgradeModal] = createSignal<{ platform: string; below: string; latest: string } | null>(null);

  // 全部设备 tab — 后端分页 + 搜索 + 平台过滤
  const [allPage, setAllPage] = createSignal(1);
  const [allQ, setAllQ] = createSignal('');
  const [allPlat, setAllPlat] = createSignal<'' | PlatKey>('');

  // 实时连接 / 近期活跃(5s 轮询)
  const [clients, { refetch: refetchClients }] = createResource(() => adminApi.getClients());
  const [dist, { refetch: refetchDist }] = createResource(() => adminApi.getClientsDistribution());
  const [gate, { refetch: refetchGate }] = createResource(() => adminApi.getVersionGate());
  // m054:关联风控复核列表(封禁时被牵连的设备)
  const [flagged, { refetch: refetchFlagged }] = createResource(() => adminApi.listFlaggedClients());

  // 全部设备分页查询(仅在 all tab 时拉)
  const [allDev, { refetch: refetchAllDev }] = createResource(
    () => (tab() === 'all' ? { page: allPage(), q: allQ(), platform: allPlat() } : null),
    (dep) => adminApi.getClientsPaginated({ page: dep.page, perPage: 14, q: dep.q || undefined, platform: dep.platform || undefined }),
  );

  // 5s 轮询实时连接列表
  const timer = setInterval(() => { void startTransition(() => refetchClients()); }, 5000);
  onCleanup(() => clearInterval(timer));

  const plats = createMemo<ClientPlatformAgg[]>(() => dist()?.platforms ?? []);
  const versions = createMemo<ClientVersionAgg[]>(() => dist()?.versions ?? []);
  const policies = createMemo<ClientUpgradePolicy[]>(() => dist()?.policies ?? []);
  const totalDev = createMemo(() => plats().reduce((a, b) => a + b.total, 0));
  const totalActive = createMemo(() => plats().reduce((a, b) => a + b.active7d, 0));

  const liveRows = createMemo<SseLiveEntry[]>(() => clients()?.sseLive ?? []);
  const recentRows = createMemo<RecentlyActiveEntry[]>(() => clients()?.recentlyActive ?? []);
  const flaggedRows = createMemo<FlaggedClientEntry[]>(() => flagged()?.flagged ?? []);

  const doBan = async () => {
    const t = banTarget();
    if (!t) return;
    try {
      if (t.action === 'ban') {
        const r = await adminApi.banClient(t.id, banReason() || undefined);
        // m054:封禁联动关联标记——提示有多少台共享 IP/账号/指纹的设备进了复核队列
        if (r.flaggedRelated > 0) {
          toast.success('已封禁设备 ' + shortId(t.id), `关联标记 ${r.flaggedRelated} 台待复核`);
        } else {
          toast.success('已封禁设备 ' + shortId(t.id));
        }
      } else {
        await adminApi.unbanClient(t.id);
        toast.success('已解封 ' + shortId(t.id));
      }
      setBanTarget(null);
      setBanReason('');
      void refetchClients();
      void refetchFlagged();
      // 「全部设备」tab 的行(含 isBanned 状态与封禁/解封按钮)源自 allDev,封禁/解封后需同步刷新。
      void refetchAllDev();
    } catch (e: any) {
      toast.error(`${t.action === 'ban' ? '封禁' : '解封'}失败`, e?.message);
    }
  };

  // m054:复核判定误报,解除单设备关联标记
  const clearFlag = async (id: string) => {
    try {
      await adminApi.clearClientFlag(id);
      toast.success('已解除关联标记 ' + shortId(id));
      void refetchFlagged();
      void refetchClients();
      void refetchAllDev();
    } catch (e: any) {
      toast.error('解除标记失败', e?.message);
    }
  };

  const reqTel = async (id: string) => {
    try {
      await adminApi.requestTelemetry(id);
      toast.success('已向 ' + shortId(id) + ' 下发遥测拉取请求');
    } catch (e: any) {
      toast.error('遥测拉取失败', e?.message);
    }
  };

  // 导出当前页 CSV
  const exportCsv = () => {
    const rows = allDev()?.data ?? [];
    if (!rows.length) { toast.info('当前页无数据可导出'); return; }
    const header = ['deviceId', 'platform', 'userId', 'appVersion', 'model', 'country', 'firstSeenAt', 'lastSeenAt', 'isBanned'];
    const esc = (v: unknown) => {
      let s = v == null ? '' : String(v);
      // CSV/Excel 公式注入防护:客户端可控字段(如 x-app-version 来源的 appVersion)可能以
      // = + - @ 或 TAB/CR 开头,被 Excel/WPS 当作公式求值。前缀单引号阻断公式执行。
      if (/^[=+\-@\t\r]/.test(s)) s = "'" + s;
      return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    };
    const lines = [header.join(','), ...rows.map((r) => header.map((k) => esc((r as unknown as Record<string, unknown>)[k])).join(','))];
    const blob = new Blob([lines.join('\n')], { type: 'text/csv;charset=utf-8;' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `devices-page-${allPage()}-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(a.href);
    toast.success('已导出当前页 CSV');
  };

  // 当前 tab 的统一表格行
  const currentRows = createMemo<DeviceRow[]>(() => {
    if (tab() === 'sse') return liveRows();
    if (tab() === 'recent') return recentRows();
    return (allDev()?.data ?? []) as ListedDevice[];
  });

  const colSpan = () => (tab() === 'sse' ? 8 : 9);
  const tableLoading = () => (tab() === 'all' ? allDev.loading : clients.loading);

  return (
    <div class="wf-devices">
      <PageHead
        title="设备与遥测"
        desc={`${fmtNum(totalDev())} 个绑定设备 · ${fmtNum(totalActive())} 个 7 天内在线 · 三端版本分布、强制升级与单设备遥测。`}
        right={
          <>
            <Btn variant="secondary" icon="refresh" onClick={() => { void refetchClients(); void refetchDist(); }}>刷新</Btn>
            <Btn variant="primary" icon="send" onClick={() => navigate('/admin/broadcast')}>新建推送</Btn>
          </>
        }
      />

      {/* 平台 hero 3 卡 */}
      <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: 16, marginBottom: 16 })}>
        <For each={PLATS}>
          {(name) => {
            const m = PLAT_META[name];
            const agg = () => plats().find((p) => p.platform === name) ?? { platform: name, total: 0, active7d: 0, monthOverMonthPct: 0 };
            const share = () => (totalDev() ? (agg().total / totalDev() * 100) : 0);
            const up = () => agg().monthOverMonthPct >= 0;
            return (
              <Card hover style={sx({ overflow: 'hidden' })}>
                <div style={sx({ display: 'flex', alignItems: 'center', gap: 11, marginBottom: 14 })}>
                  <span style={sx({ width: 38, height: 38, borderRadius: 11, display: 'grid', placeItems: 'center', background: `color-mix(in oklch, ${m.col} 15%, transparent)`, color: m.col })}>
                    <Icon name={m.icon} size={20} />
                  </span>
                  <div style={sx({ flex: 1, minWidth: 0 })}>
                    <div style={sx({ fontWeight: 700, fontSize: 14 })}>{m.name}</div>
                    <div class="muted-3" style={sx({ fontSize: 11 })}>{m.sub}</div>
                  </div>
                  <Badge variant={up() ? 'success' : 'error'}>{up() ? '▲' : '▼'} {Math.abs(agg().monthOverMonthPct).toFixed(0)}%</Badge>
                </div>
                <div style={sx({ display: 'flex', alignItems: 'baseline', gap: 8 })}>
                  <span class="tnum" style={sx({ fontSize: 28, fontWeight: 800 })}>{fmtNum(agg().total)}</span>
                  <span class="muted" style={sx({ fontSize: 12.5 })}>设备 · {share().toFixed(0)}%</span>
                </div>
                <div style={sx({ marginTop: 10 })}><Progress value={share()} tone="accent" height={6} /></div>
                <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 8 })}>{fmtNum(agg().active7d)} 个 7 天内在线</div>
              </Card>
            );
          }}
        </For>
      </div>

      {/* 设备表 */}
      <Card pad={false} style={sx({ overflow: 'hidden', marginBottom: 16 })}>
        <div style={sx({ padding: 14, display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap', borderBottom: '1px solid var(--hairline)' })}>
          <Tabs
            value={tab()} onChange={(v) => setTab(v as 'sse' | 'recent' | 'all' | 'flagged')}
            tabs={[
              { value: 'sse', label: '实时连接', count: liveRows().length },
              { value: 'recent', label: '近期活跃', count: recentRows().length },
              { value: 'all', label: '全部设备', count: (allQ() || allPlat()) ? (allDev()?.total ?? 0) : (totalDev() || (allDev()?.total ?? 0)) },
              { value: 'flagged', label: '风险标记', count: flaggedRows().length },
            ]}
          />
          <Show when={tab() === 'all'}>
            <div style={sx({ display: 'flex', gap: 8, marginLeft: 'auto', flexWrap: 'wrap', alignItems: 'center' })}>
              <div style={sx({ position: 'relative' })}>
                <Icon name="search" size={14} style={sx({ position: 'absolute', left: 10, top: 9, color: 'var(--text-3)' })} />
                <input class="input" style={sx({ paddingLeft: 32, width: 200 })} placeholder="设备 / 用户 ID"
                  value={allQ()} onInput={(e) => { setAllQ(e.currentTarget.value); setAllPage(1); }} />
              </div>
              <select class="select" style={sx({ width: 'auto' })} value={allPlat()}
                onChange={(e) => { setAllPlat(e.currentTarget.value as '' | PlatKey); setAllPage(1); }}>
                <option value="">全部平台</option>
                <option value="web">web</option>
                <option value="ios">ios</option>
                <option value="android">android</option>
              </select>
              <Btn size="sm" variant="secondary" icon="download" onClick={exportCsv}>导出</Btn>
            </div>
          </Show>
          <Show when={tab() === 'sse'}>
            <span class="muted-3" style={sx({ marginLeft: 'auto', display: 'inline-flex', gap: 6, alignItems: 'center', fontSize: 12 })}>
              <span class="live-dot" />实时 SSE
            </span>
          </Show>
        </div>

        {/* m054:风险标记复核表(封禁牵连的关联设备) */}
        <Show when={tab() === 'flagged'}>
          <div style={sx({ overflowX: 'auto' })}>
            <table class="tbl">
              <thead>
                <tr>
                  <th>设备 ID</th>
                  <th>平台</th>
                  <th>用户</th>
                  <th>标记原因</th>
                  <th>触发源</th>
                  <th>标记时间</th>
                  <th style={sx({ textAlign: 'right' })}>操作</th>
                </tr>
              </thead>
              <tbody>
                <Show
                  when={!flagged.loading}
                  fallback={<For each={Array.from({ length: 6 })}>{() => <tr><td colSpan={7}><Skel h={22} /></td></tr>}</For>}
                >
                  <Show
                    when={flaggedRows().length}
                    fallback={<tr><td colSpan={7}><Empty title="暂无风险标记" desc="封禁设备时,共享 IP / 账号 / 设备指纹的关联设备会出现在此待复核" icon="devices" /></td></tr>}
                  >
                    <For each={flaggedRows()}>
                      {(r) => (
                        <tr>
                          <td class="mono" style={sx({ fontSize: 11.5 })} title={r.deviceId}>{shortId(r.deviceId)}</td>
                          <td><Badge variant="default">{r.platform}</Badge></td>
                          <td class="mono" style={sx({ fontSize: 11.5 })}>{shortId(r.userId)}</td>
                          <td class="muted" style={sx({ fontSize: 12, maxWidth: 260, whiteSpace: 'normal' })}>{r.riskReason ?? '—'}</td>
                          <td class="mono" style={sx({ fontSize: 11.5 })} title={r.riskRelatedDevice ?? ''}>{shortId(r.riskRelatedDevice)}</td>
                          <td class="muted" style={sx({ fontSize: 12 })}>{r.riskFlaggedAt ? fmtAgo(r.riskFlaggedAt) : '—'}</td>
                          <td>
                            <div style={sx({ display: 'flex', gap: 5, justifyContent: 'flex-end', flexWrap: 'wrap' })}>
                              <Show when={!r.isBanned} fallback={<Badge variant="error">已封禁</Badge>}>
                                <Btn size="xs" variant="danger" onClick={() => setBanTarget({ id: r.deviceId, action: 'ban' })}>封禁</Btn>
                              </Show>
                              <Btn size="xs" variant="outline" onClick={() => clearFlag(r.deviceId)}>解除标记</Btn>
                              <Btn size="xs" variant="ghost" onClick={() => setDetail(r.deviceId)}>遥测详情</Btn>
                            </div>
                          </td>
                        </tr>
                      )}
                    </For>
                  </Show>
                </Show>
              </tbody>
            </table>
          </div>
        </Show>

        <Show when={tab() !== 'flagged'}>
        <div style={sx({ overflowX: 'auto' })}>
          <table class="tbl">
            <thead>
              <tr>
                <th>设备 ID</th>
                <th>平台</th>
                <th>版本</th>
                <th>用户</th>
                <Show when={tab() !== 'sse'}><th>国家</th></Show>
                <Show when={tab() === 'sse'} fallback={<th>最后活跃</th>}>
                  <th>连接时长</th>
                  <th>连接数</th>
                </Show>
                <Show when={tab() !== 'all'}><th>数据状态</th></Show>
                <th style={sx({ textAlign: 'right' })}>操作</th>
              </tr>
            </thead>
            <tbody>
              <Show
                when={!tableLoading()}
                fallback={
                  <For each={Array.from({ length: 6 })}>
                    {() => <tr><td colSpan={colSpan()}><Skel h={22} /></td></tr>}
                  </For>
                }
              >
                <Show
                  when={currentRows().length}
                  fallback={<tr><td colSpan={colSpan()}><Empty title="暂无设备" desc="当前筛选条件下没有匹配的设备" icon="devices" /></td></tr>}
                >
                  <For each={currentRows()}>
                    {(r) => (
                      <tr>
                        <td class="mono" style={sx({ fontSize: 11.5 })} title={r.deviceId}>
                          <span style={sx({ display: 'inline-flex', alignItems: 'center', gap: 6 })}>
                            {shortId(r.deviceId)}
                            <Show when={r.riskFlag}>
                              <span title={r.riskRelatedDevice ? `关联自 ${r.riskRelatedDevice}` : '关联风险标记'}>
                                <Badge variant="warning">风险</Badge>
                              </span>
                            </Show>
                          </span>
                        </td>
                        <td><Badge variant="default">{r.platform}</Badge></td>
                        <td class="mono muted" style={sx({ fontSize: 11.5 })}>{r.appVersion ?? '—'}</td>
                        <td class="mono" style={sx({ fontSize: 11.5 })}>{shortId(r.userId)}</td>
                        <Show when={tab() !== 'sse'}>
                          <td class="muted" style={sx({ fontSize: 12 })}>{r.country ?? '—'}</td>
                        </Show>
                        <Show
                          when={tab() === 'sse'}
                          fallback={<td class="muted" style={sx({ fontSize: 12 })}>{r.lastSeenAt ? fmtAgo(r.lastSeenAt) : '—'}</td>}
                        >
                          <td class="tnum">{Math.floor((r.connectedSecs ?? 0) / 60)}m</td>
                          <td class="tnum">{r.connectionCount ?? 0}</td>
                        </Show>
                        <Show when={tab() !== 'all'}>
                          <td>{r.dataChannels ? <ChanBadges ch={r.dataChannels} /> : '—'}</td>
                        </Show>
                        <td>
                          <div style={sx({ display: 'flex', gap: 5, justifyContent: 'flex-end', flexWrap: 'wrap' })}>
                            <Btn size="xs" variant={r.isBanned ? 'success' : 'danger'}
                              onClick={() => setBanTarget({ id: r.deviceId, action: r.isBanned ? 'unban' : 'ban' })}>
                              {r.isBanned ? '解封' : '封禁'}
                            </Btn>
                            <Show when={tab() === 'sse'}>
                              <Btn size="xs" variant="outline" onClick={() => reqTel(r.deviceId)}>拉取遥测</Btn>
                            </Show>
                            <Btn size="xs" variant="ghost" onClick={() => setDetail(r.deviceId)}>遥测详情</Btn>
                          </div>
                        </td>
                      </tr>
                    )}
                  </For>
                </Show>
              </Show>
            </tbody>
          </table>
        </div>

        <Show when={tab() === 'all'}>
          <div style={sx({ padding: 12 })}>
            <Pagination page={allPage()} totalPages={allDev()?.totalPages ?? 1} onPage={setAllPage} />
          </div>
        </Show>
        </Show>
      </Card>

      {/* 版本分布 + 升级策略 + 版本门控 */}
      <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.3fr) minmax(0,1fr)', gap: 16 })}>
        <Panel title="版本分布与升级策略" sub="按平台配置最低 / 建议版本与灰度比例">
          <Show when={dist.state !== 'pending'} fallback={<Loading />}>
            <Show when={policies().length} fallback={<Empty title="暂无升级策略" desc="等待平台聚合数据" icon="cpu" />}>
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
                <For each={policies()}>
                  {(p) => (
                    <PolicyEditor
                      policy={p}
                      versions={versions().filter((v) => v.platform === p.platform)}
                      onSaved={() => void refetchDist()}
                      onForce={(below, latest) => setUpgradeModal({ platform: p.platform, below, latest })}
                    />
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </Panel>

        <Panel title="全局版本门控" sub="strict-mode · 即时生效">
          <Show when={!gate.loading && gate()} fallback={<Loading />}>
            {(g) => <VersionGateEditor gate={g()} onSaved={() => void refetchGate()} />}
          </Show>
        </Panel>
      </div>

      {/* 遥测抽屉 */}
      <DeviceDrawer
        deviceId={detail()}
        onClose={() => setDetail(null)}
        onBan={(id, action) => setBanTarget({ id, action })}
        onClearFlag={clearFlag}
      />

      {/* 封禁 / 解封确认 */}
      <Modal
        open={!!banTarget()} onClose={() => setBanTarget(null)}
        title={banTarget()?.action === 'ban' ? '封禁设备' : '解封设备'} size="sm"
        footer={
          <>
            <Btn variant="ghost" onClick={() => setBanTarget(null)}>取消</Btn>
            <Btn variant={banTarget()?.action === 'ban' ? 'danger' : 'success'} onClick={doBan}>
              确认{banTarget()?.action === 'ban' ? '封禁' : '解封'}
            </Btn>
          </>
        }
      >
        <p class="muted" style={sx({ fontSize: 13, marginTop: 0 })}>设备 <span class="mono">{shortId(banTarget()?.id)}</span></p>
        <Show when={banTarget()?.action === 'ban'}>
          <Field label="封禁原因(可选)">
            <input class="input" value={banReason()} onInput={(e) => setBanReason(e.currentTarget.value)} placeholder="如:异常高频请求" />
          </Field>
        </Show>
      </Modal>

      <ForceUpgradeModal target={upgradeModal()} onClose={() => setUpgradeModal(null)} />
    </div>
  );
}
