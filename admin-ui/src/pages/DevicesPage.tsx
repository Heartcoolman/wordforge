import { batch, createMemo, createSignal, onMount, Show, For } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { HeroCard } from '@/components/ui/HeroCard';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Input } from '@/components/ui/Input';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Tabs } from '@/components/ui/Tabs';
import { Table } from '@/components/ui/Table';
import { adminApi, type SseLiveEntry, type RecentlyActiveEntry, type TelemetrySummary, type DataChannelValue } from '@/api/admin';
import { uiStore } from '@/stores/ui';

const CHANNEL_LABELS: Record<string, string> = {
  amas: 'AMAS',
  learning: '学习',
  telemetry: '遥测',
};

const CHANNEL_VARIANT: Record<DataChannelValue, 'success' | 'warning' | 'error'> = {
  uploaded: 'success',
  nil: 'warning',
  none: 'error',
};

const STATUS_TEXT: Record<DataChannelValue, string> = {
  uploaded: '已上传',
  nil: '空数据',
  none: '未上传',
};

// 时间戳格式化：兼容 "YYYY-MM-DD HH:MM:SS"（后端老格式）和 ISO 8601；空值/解析失败回退原串
const formatTimestamp = (s: string | null | undefined): string => {
  if (!s) return '';
  const iso = s.includes('T') ? s : s.replace(' ', 'T') + 'Z';
  const d = new Date(iso);
  return isNaN(d.getTime()) ? s : d.toLocaleString();
};

function DataChannelBadge(props: { channel: string; status: DataChannelValue }) {
  return (
    <Badge
      variant={CHANNEL_VARIANT[props.status]}
      size="sm"
      dot
      title={`${CHANNEL_LABELS[props.channel]}: ${STATUS_TEXT[props.status]}`}
    >
      {CHANNEL_LABELS[props.channel]}
    </Badge>
  );
}

export default function DevicesPage() {
  const [sseLive, setSseLive] = createSignal<SseLiveEntry[]>([]);
  const [recentlyActive, setRecentlyActive] = createSignal<RecentlyActiveEntry[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [tab, setTab] = createSignal<'sse' | 'recent'>('sse');

  // Ban confirm
  const [banTarget, setBanTarget] = createSignal<{ id: string; action: 'ban' | 'unban' } | null>(null);
  const [banReason, setBanReason] = createSignal('');

  // Telemetry
  const [telemetryDevice, setTelemetryDevice] = createSignal<string | null>(null);
  const [telemetryRecords, setTelemetryRecords] = createSignal<TelemetrySummary[]>([]);
  const [telemetryTotal, setTelemetryTotal] = createSignal(0);
  const [telemetryLoading, setTelemetryLoading] = createSignal(false);
  // 正在请求遥测的 deviceId，避免重复点击"拉取遥测"
  const [requestingTelemetry, setRequestingTelemetry] = createSignal<string | null>(null);
  let telemetryRequestId = 0;

  const loadDevices = async () => {
    try {
      setLoading(true);
      const data = await adminApi.getClients();
      setSseLive(data.sseLive);
      setRecentlyActive(data.recentlyActive);
    } catch (e: any) {
      uiStore.toast.error('加载设备列表失败', e.message);
    } finally {
      setLoading(false);
    }
  };

  const handleBan = async () => {
    const t = banTarget();
    if (!t) return;
    const shortId = truncateId(t.id);
    try {
      if (t.action === 'ban') {
        await adminApi.banClient(t.id, banReason() || undefined);
        uiStore.toast.success(`已封禁设备 ${shortId}`);
      } else {
        await adminApi.unbanClient(t.id);
        uiStore.toast.success(`已解封设备 ${shortId}`);
      }
      setBanTarget(null);
      setBanReason('');
      loadDevices();
    } catch (e: any) {
      uiStore.toast.error(`${t.action === 'ban' ? '封禁' : '解封'} ${shortId} 失败`, e.message);
    }
  };

  const requestTelemetry = async (deviceId: string) => {
    const shortId = truncateId(deviceId);
    setRequestingTelemetry(deviceId);
    try {
      await adminApi.requestTelemetry(deviceId);
      uiStore.toast.success(`已向 ${shortId} 发送遥测请求`);
    } catch (e: any) {
      uiStore.toast.error(`向 ${shortId} 请求遥测失败`, e.message);
    } finally {
      setRequestingTelemetry(null);
    }
  };

  const loadTelemetry = async (deviceId: string) => {
    const requestId = ++telemetryRequestId;
    try {
      // 一次性切换设备 + 清空旧数据 + 进入加载态，避免三段渲染闪烁
      batch(() => {
        setTelemetryDevice(deviceId);
        setTelemetryRecords([]);
        setTelemetryTotal(0);
        setTelemetryLoading(true);
      });
      const data = await adminApi.getTelemetry(deviceId);
      if (requestId !== telemetryRequestId) return;
      batch(() => {
        setTelemetryRecords(data.records);
        setTelemetryTotal(data.total);
      });
    } catch (e: any) {
      if (requestId !== telemetryRequestId) return;
      uiStore.toast.error(`加载 ${truncateId(deviceId)} 遥测数据失败`, e.message);
    } finally {
      if (requestId === telemetryRequestId) {
        setTelemetryLoading(false);
      }
    }
  };

  onMount(loadDevices);

  function truncateId(id: string | null | undefined) {
    if (!id) return '';
    return id.length > 12 ? `${id.slice(0, 8)}...${id.slice(-4)}` : id;
  }

  // 平台分布：合并 SSE + Recent 去重后按 platform 聚合(版本字段后端未透出,以平台分布替代)
  const platformDistribution = createMemo<Array<{ platform: string; count: number; share: number }>>(() => {
    const seen = new Set<string>();
    const byPlatform = new Map<string, number>();
    const collect = (entries: Array<{ deviceId: string; platform: string }>) => {
      for (const e of entries) {
        if (seen.has(e.deviceId)) continue;
        seen.add(e.deviceId);
        byPlatform.set(e.platform, (byPlatform.get(e.platform) ?? 0) + 1);
      }
    };
    collect(sseLive());
    collect(recentlyActive());
    const total = Array.from(byPlatform.values()).reduce((a, b) => a + b, 0) || 1;
    return Array.from(byPlatform.entries())
      .map(([platform, count]) => ({ platform, count, share: count / total }))
      .sort((a, b) => b.count - a.count);
  });

  const platformBarColor: Record<string, string> = {
    web: 'var(--accent)',
    ios: 'var(--info)',
    android: 'var(--success)',
  };

  // SSE 表列定义
  const sseColumns = [
    {
      key: 'deviceId',
      title: '设备 ID',
      class: 'whitespace-nowrap',
      render: (r: SseLiveEntry) => (
        <span class="font-mono text-xs tabular-nums" title={r.deviceId}>{truncateId(r.deviceId)}</span>
      ),
    },
    {
      key: 'platform',
      title: '平台',
      render: (r: SseLiveEntry) => (
        <Badge variant="default" size="sm">{r.platform}</Badge>
      ),
    },
    {
      key: 'userId',
      title: '用户',
      render: (r: SseLiveEntry) => (
        <span class="font-mono text-xs tabular-nums">{truncateId(r.userId)}</span>
      ),
    },
    {
      key: 'connectedSecs',
      title: '连接时长',
      render: (r: SseLiveEntry) => <span class="tabular-nums">{Math.floor(r.connectedSecs / 60)}m</span>,
    },
    {
      key: 'connectionCount',
      title: '连接数',
      render: (r: SseLiveEntry) => <span class="tabular-nums">{r.connectionCount}</span>,
    },
    {
      key: 'channels',
      title: '数据状态',
      render: (r: SseLiveEntry) => (
        <div class="flex flex-wrap gap-1">
          <DataChannelBadge channel="amas" status={r.dataChannels.amas} />
          <DataChannelBadge channel="learning" status={r.dataChannels.learning} />
          <DataChannelBadge channel="telemetry" status={r.dataChannels.telemetry} />
        </div>
      ),
    },
    {
      key: 'actions',
      title: '操作',
      render: (r: SseLiveEntry) => (
        <div class="flex gap-1 flex-wrap">
          <Show when={r.isBanned} fallback={
            <Button size="xs" variant="danger" disabled={loading()} onClick={() => setBanTarget({ id: r.deviceId, action: 'ban' })}>封禁</Button>
          }>
            <Button size="xs" variant="success" disabled={loading()} onClick={() => setBanTarget({ id: r.deviceId, action: 'unban' })}>解封</Button>
          </Show>
          <Button size="xs" variant="outline" disabled={requestingTelemetry() === r.deviceId} onClick={() => requestTelemetry(r.deviceId)}>拉取遥测</Button>
          <Button size="xs" variant="ghost" onClick={() => loadTelemetry(r.deviceId)}>历史</Button>
        </div>
      ),
    },
  ];

  // Recent 表列定义(无 connection 字段,加状态列)
  const recentColumns = [
    {
      key: 'deviceId',
      title: '设备 ID',
      class: 'whitespace-nowrap',
      render: (r: RecentlyActiveEntry) => (
        <span class="font-mono text-xs tabular-nums" title={r.deviceId}>{truncateId(r.deviceId)}</span>
      ),
    },
    {
      key: 'platform',
      title: '平台',
      render: (r: RecentlyActiveEntry) => <Badge variant="default" size="sm">{r.platform}</Badge>,
    },
    {
      key: 'userId',
      title: '用户',
      render: (r: RecentlyActiveEntry) => (
        <span class="font-mono text-xs tabular-nums">{r.userId ? truncateId(r.userId) : '-'}</span>
      ),
    },
    {
      key: 'lastSeenAt',
      title: '最后活跃',
      render: (r: RecentlyActiveEntry) => (
        <span class="text-xs tabular-nums whitespace-nowrap">{formatTimestamp(r.lastSeenAt)}</span>
      ),
    },
    {
      key: 'status',
      title: '状态',
      render: (r: RecentlyActiveEntry) => (
        <Badge variant={r.isBanned ? 'error' : 'success'} size="sm" dot>{r.isBanned ? '已封禁' : '正常'}</Badge>
      ),
    },
    {
      key: 'channels',
      title: '数据状态',
      render: (r: RecentlyActiveEntry) => (
        <div class="flex flex-wrap gap-1">
          <DataChannelBadge channel="amas" status={r.dataChannels.amas} />
          <DataChannelBadge channel="learning" status={r.dataChannels.learning} />
          <DataChannelBadge channel="telemetry" status={r.dataChannels.telemetry} />
        </div>
      ),
    },
    {
      key: 'actions',
      title: '操作',
      render: (r: RecentlyActiveEntry) => (
        <div class="flex gap-1 flex-wrap">
          <Show when={r.isBanned} fallback={
            <Button size="xs" variant="danger" disabled={loading()} onClick={() => setBanTarget({ id: r.deviceId, action: 'ban' })}>封禁</Button>
          }>
            <Button size="xs" variant="success" disabled={loading()} onClick={() => setBanTarget({ id: r.deviceId, action: 'unban' })}>解封</Button>
          </Show>
          <Button size="xs" variant="ghost" onClick={() => loadTelemetry(r.deviceId)}>历史</Button>
        </div>
      ),
    },
  ];

  return (
    <div class="space-y-6">
      <HeroCard
        eyebrow="SSE + Telemetry"
        eyebrowVariant="info"
        title="设备管理"
        desc="所有通过 /api/* 接入后端的 end-user 设备（Web / iOS / Android），含实时 SSE 连接与接入历史统计。"
        meta={[
          { value: sseLive().length, label: '在线连接数' },
          { value: recentlyActive().length, label: '近期接入设备' },
        ]}
        cta={<Button size="sm" variant="outline" onClick={loadDevices} disabled={loading()}>刷新</Button>}
      />

      {/* Tabs / Telemetry Panel 不受 loading 影响：刷新时 Telemetry 已展开面板不被卸载 */}
      <Tabs
        tabs={[
          { id: 'sse', label: `SSE 实时连接 (${sseLive().length})` },
          { id: 'recent', label: `近期活跃 (${recentlyActive().length})` },
        ]}
        active={tab()}
        onChange={(id) => setTab(id as 'sse' | 'recent')}
      />

      {/* 平台分布 mini bar:版本字段后端未透出,以接入平台分布替代 */}
      <Show when={platformDistribution().length > 0}>
        <Card>
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold text-content">平台分布</h3>
            <span class="text-xs text-content-tertiary tabular-nums">
              共 {platformDistribution().reduce((a, b) => a + b.count, 0)} 台已知设备
            </span>
          </div>
          <ul class="space-y-2">
            <For each={platformDistribution()}>
              {(row) => (
                <li class="grid grid-cols-[80px_1fr_auto] items-center gap-3 text-[12.5px]">
                  <span class="font-mono text-content-secondary truncate uppercase tracking-wide" title={row.platform}>{row.platform.toUpperCase()}</span>
                  <div class="h-2 rounded-full bg-surface-secondary overflow-hidden">
                    <div
                      class="h-full rounded-full transition-[width] duration-base ease-out-expo"
                      style={{
                        width: `${(row.share * 100).toFixed(1)}%`,
                        background: platformBarColor[row.platform.toLowerCase()] ?? 'var(--accent)',
                      }}
                    />
                  </div>
                  <span class="font-mono tabular-nums text-content">
                    {row.count} <span class="text-content-tertiary">({(row.share * 100).toFixed(0)}%)</span>
                  </span>
                </li>
              )}
            </For>
          </ul>
        </Card>
      </Show>

      {/* 列表/表格区单独 loading 边界 */}
      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        {/* SSE Live */}
        <Show when={tab() === 'sse'}>
          <Show
            when={sseLive().length > 0}
            fallback={<Card variant="outlined" padding="lg"><Empty title="暂无活跃 SSE 连接" description="实时连接数为 0；用户登录后会出现在这里" /></Card>}
          >
            <Table
              columns={sseColumns}
              data={sseLive()}
              aria-label="SSE 实时连接"
              caption="按设备 ID / 平台 / 用户 / 连接时长 / 连接数 / 数据上报状态 列展示活跃 SSE 连接"
            />
          </Show>
        </Show>

        {/* Recently Active */}
        <Show when={tab() === 'recent'}>
          <Show
            when={recentlyActive().length > 0}
            fallback={<Card variant="outlined" padding="lg"><Empty title="暂无近期活跃设备" description="最近 24h 内没有设备上报过活动" /></Card>}
          >
            <Table
              columns={recentColumns}
              data={recentlyActive()}
              aria-label="近期活跃设备"
              caption="按设备 ID / 平台 / 用户 / 最后活跃 / 封禁状态 / 数据上报状态 列展示最近接入设备"
            />
          </Show>
        </Show>
      </Show>

      {/* Telemetry Panel — 不被刷新 loading 卸载，避免已展开的遥测面板因主列表刷新而消失 */}
      <Show when={telemetryDevice()}>
        <Card class="mt-4">
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold">遥测记录 — <span class="font-mono">{truncateId(telemetryDevice()!)}</span> (共 {telemetryTotal()} 条)</h3>
            <Button size="xs" variant="ghost" onClick={() => setTelemetryDevice(null)}>关闭</Button>
          </div>
          <Show when={!telemetryLoading()} fallback={<div class="flex justify-center py-4"><Spinner /></div>}>
            <Show when={telemetryRecords().length > 0} fallback={<Empty title="暂无遥测记录" description="该设备尚未上传遥测数据" />}>
              <div class="space-y-2 max-h-80 overflow-y-auto">
                <For each={telemetryRecords()}>
                    {(record) => (
                      <Card variant="outlined" padding="sm" class="text-xs space-y-2">
                        <div class="flex justify-between items-center text-content-secondary gap-2 min-w-0">
                          <Badge variant="default" size="sm">{record.eventType}</Badge>
                          <span class="tabular-nums whitespace-nowrap">{formatTimestamp(record.serverTs)}</span>
                        </div>
                        {/* 设备信息 */}
                        <Show when={record.deviceProfile.osName || record.deviceProfile.browserName}>
                          <div class="bg-surface-secondary rounded p-1.5 space-y-0.5">
                            <div class="font-semibold text-info mb-1 flex items-center gap-1.5">
                              <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                              </svg>
                              设备信息
                            </div>
                            <Show when={record.deviceProfile.osName}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">系统</span><span>{record.deviceProfile.osName}</span></div>
                            </Show>
                            <Show when={record.deviceProfile.browserName}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">浏览器</span><span>{record.deviceProfile.browserName} {record.deviceProfile.browserVersion}</span></div>
                            </Show>
                            <Show when={record.deviceProfile.cpuCores !== null}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">CPU</span><span>{record.deviceProfile.cpuCores} 核{record.deviceProfile.memoryGb ? ` / ${record.deviceProfile.memoryGb}GB` : ''}</span></div>
                            </Show>
                            <Show when={record.deviceProfile.screenWidth !== null}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">分辨率</span><span>{record.deviceProfile.screenWidth}×{record.deviceProfile.screenHeight} @{record.deviceProfile.pixelRatio}x</span></div>
                            </Show>
                            <Show when={record.deviceProfile.timezone}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">时区</span><span>{record.deviceProfile.timezone} / {record.deviceProfile.language}</span></div>
                            </Show>
                          </div>
                        </Show>
                        {/* 会话统计 */}
                        <div class="bg-surface-secondary rounded p-1.5 space-y-0.5">
                          <div class="font-semibold text-accent mb-1 flex items-center gap-1.5">
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            会话统计
                          </div>
                          <div class="flex gap-4 flex-wrap">
                            <span><span class="text-content-tertiary">时长 </span>{record.sessionStats.sessionDurationSecs}s</span>
                            <span><span class="text-content-tertiary">操作/分 </span>{record.sessionStats.actionsPerMin.toFixed(1)}</span>
                            <span><span class="text-content-tertiary">错误 </span>{record.sessionStats.errorCount}</span>
                            <span><span class="text-content-tertiary">响应 </span>{record.sessionStats.avgResponseTimeMs.toFixed(0)}ms</span>
                          </div>
                        </div>
                        {/* 行为摘要 */}
                        <Show when={record.behaviorSummary.currentRoute !== null}>
                          <div class="bg-surface-secondary rounded p-1.5 space-y-0.5">
                            <div class="font-semibold text-success mb-1 flex items-center gap-1.5">
                              <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M15 15l-2 5L9 9l11 4-5 2zm0 0l5 5M7.188 2.239l.777 2.897M5.136 7.965l-2.898-.777M13.95 4.05l-2.122 2.122m-5.657 5.656l-2.12 2.122" />
                              </svg>
                              行为摘要
                            </div>
                            <Show when={record.behaviorSummary.currentRoute}>
                              <div class="flex gap-2"><span class="text-content-tertiary w-12">路由</span><span class="font-mono">{record.behaviorSummary.currentRoute}</span></div>
                            </Show>
                            <div class="flex gap-4 flex-wrap">
                              <Show when={record.behaviorSummary.clickCount !== null}>
                                <span><span class="text-content-tertiary">点击 </span>{record.behaviorSummary.clickCount}</span>
                              </Show>
                              <Show when={record.behaviorSummary.scrollDepthPct !== null}>
                                <span><span class="text-content-tertiary">滚动 </span>{record.behaviorSummary.scrollDepthPct!.toFixed(0)}%</span>
                              </Show>
                              <Show when={record.behaviorSummary.routeChanges !== null}>
                                <span><span class="text-content-tertiary">跳转 </span>{record.behaviorSummary.routeChanges}</span>
                              </Show>
                              <Show when={record.behaviorSummary.visibilityChanges !== null}>
                                <span><span class="text-content-tertiary">焦点变更 </span>{record.behaviorSummary.visibilityChanges}</span>
                              </Show>
                            </div>
                          </div>
                        </Show>
                        {/* 功能使用 */}
                        <Show when={Object.keys(record.featureUsage).length > 0}>
                          <div class="bg-surface-secondary rounded p-1.5">
                            <div class="font-semibold text-warning mb-1 flex items-center gap-1.5">
                              <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z" />
                              </svg>
                              功能使用
                            </div>
                            <div class="flex gap-3 flex-wrap">
                              <For each={Object.entries(record.featureUsage)}>
                                {([k, v]) => <span class="tabular-nums"><span class="text-content-tertiary">{k} </span>{v}</span>}
                              </For>
                            </div>
                          </div>
                        </Show>
                      </Card>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </Card>
        </Show>

      {/* Ban Confirm Dialog */}
      <Show when={banTarget()}>
        {(target) => (
          <ConfirmDialog
            open={true}
            title={target().action === 'ban' ? '确认封禁设备' : '确认解封设备'}
            message={<>设备 ID: <span class="font-mono">{truncateId(target().id)}</span></>}
            confirmText={target().action === 'ban' ? '确认封禁' : '确认解封'}
            variant={target().action === 'ban' ? 'danger' : 'success'}
            onConfirm={handleBan}
            onCancel={() => { setBanTarget(null); setBanReason(''); }}
          >
            <Show when={target().action === 'ban'}>
              <Input
                type="text"
                placeholder="封禁原因（可选）"
                value={banReason()}
                onInput={(e) => setBanReason(e.currentTarget.value)}
                maxlength={500}
              />
            </Show>
          </ConfirmDialog>
        )}
      </Show>
    </div>
  );
}
