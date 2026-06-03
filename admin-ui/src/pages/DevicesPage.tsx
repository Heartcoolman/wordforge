import { batch, createEffect, createMemo, createSignal, onMount, Show, For } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Input } from '@/components/ui/Input';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Tabs } from '@/components/ui/Tabs';
import { Table } from '@/components/ui/Table';
import { adminApi, type SseLiveEntry, type RecentlyActiveEntry, type TelemetrySummary, type TelemetryDeviceSummary, type DataChannelValue } from '@/api/admin';
import { ApiError } from '@/api/http';
import type { ClientPlatformAgg, ClientVersionAgg, ClientUpgradePolicy, ListedDevice, VersionGate, ScheduledBroadcast } from '@/types/admin';
import { uiStore } from '@/stores/ui';
import { Switch } from '@/components/ui/Switch';
import { DeviceDetailDrawer, type DeviceDetailSeed } from './devices/DeviceDetailDrawer';

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

// 秒 → 人类可读时长(总活跃时长展示)
const fmtDuration = (secs: number): string => {
  if (!secs || secs < 60) return `${Math.round(secs || 0)}s`;
  const h = Math.floor(secs / 3600);
  const m = Math.round((secs % 3600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
};

// 事件 type → 系统分类(与探针看板口径一致):learn/perf/err/behavior
type EvtCategory = 'learn' | 'perf' | 'err' | 'behavior';
function evtCategory(type: string): EvtCategory {
  const t = type.toLowerCase();
  if (t.includes('error') || t.includes('err')) return 'err';
  if (t.includes('lesson') || t.includes('word') || t.includes('answer') || t.includes('session') || t.includes('review')) return 'learn';
  if (t.includes('perf') || t.includes('resource') || t.includes('nav')) return 'perf';
  return 'behavior';
}
const CAT_VARIANT: Record<EvtCategory, 'success' | 'info' | 'error' | 'accent'> = {
  learn: 'success', perf: 'info', err: 'error', behavior: 'accent',
};
const CAT_DOT: Record<EvtCategory, string> = {
  learn: 'bg-success', perf: 'bg-info', err: 'bg-error', behavior: 'bg-accent',
};
const CAT_LABEL: Record<EvtCategory, string> = {
  learn: '学习', perf: '性能', err: '错误', behavior: '行为',
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
  const [tab, setTab] = createSignal<'sse' | 'recent' | 'all'>('sse');

  // m032:单设备详情抽屉 — 任意行"详情"按钮打开,带入行快照 + 抽屉内拉遥测补全
  const [detailDevice, setDetailDevice] = createSignal<DeviceDetailSeed | null>(null);

  // Ban confirm
  const [banTarget, setBanTarget] = createSignal<{ id: string; action: 'ban' | 'unban' } | null>(null);
  const [banReason, setBanReason] = createSignal('');

  // m027:平台聚合 + 版本分布 + 升级策略(对齐 clients.html 设计图)
  const [platforms, setPlatforms] = createSignal<ClientPlatformAgg[]>([]);
  const [versionRows, setVersionRows] = createSignal<ClientVersionAgg[]>([]);
  const [policies, setPolicies] = createSignal<ClientUpgradePolicy[]>([]);
  const [versionFilter, setVersionFilter] = createSignal<'all' | 'web' | 'ios' | 'android'>('all');

  // m027:升级策略编辑态(per-platform draft;保存后 invalidate)
  const [policyDraft, setPolicyDraft] = createSignal<Record<string, Partial<ClientUpgradePolicy>>>({});
  const [savingPolicy, setSavingPolicy] = createSignal<string | null>(null);

  // D4:全局客户端最低版本门控(strict-mode 运行时,即时生效)
  const [versionGate, setVersionGate] = createSignal<VersionGate | null>(null);
  const [vgEnabled, setVgEnabled] = createSignal(false);
  const [vgMinVersion, setVgMinVersion] = createSignal('');
  const [vgSaving, setVgSaving] = createSignal(false);
  // 合法 semver(\d+.\d+.\d+) 前端预校验,把非法值拦在请求前
  const VG_SEMVER_RE = /^\d+\.\d+\.\d+(?:[-+].+)?$/;
  const vgVersionInvalid = createMemo(() => {
    const v = vgMinVersion().trim();
    return v.length > 0 && !VG_SEMVER_RE.test(v);
  });
  const vgDirty = createMemo(() => {
    const g = versionGate();
    if (!g) return false;
    return vgEnabled() !== g.enabled || vgMinVersion().trim() !== (g.minClientVersion ?? '');
  });

  // m027:强制升级广播 modal
  const [broadcastUpgradeOpen, setBroadcastUpgradeOpen] = createSignal<{ platform: string; below: string; latest: string } | null>(null);
  const [broadcastUpgradeMessage, setBroadcastUpgradeMessage] = createSignal('');
  const [broadcastUpgradeSending, setBroadcastUpgradeSending] = createSignal(false);

  // m027 / m027-G:全部设备 tab — 后端分页 + 搜索 + CSV
  const [allPage, setAllPage] = createSignal(1);
  const [allPerPage] = createSignal(20);
  const [allQ, setAllQ] = createSignal('');
  const [allPlatform, setAllPlatform] = createSignal<'' | 'web' | 'ios' | 'android'>('');
  const [allRows, setAllRows] = createSignal<ListedDevice[]>([]);
  const [allTotal, setAllTotal] = createSignal(0);
  const [allLoading, setAllLoading] = createSignal(false);

  // Telemetry
  const [telemetryDevice, setTelemetryDevice] = createSignal<string | null>(null);
  const [telemetryRecords, setTelemetryRecords] = createSignal<TelemetrySummary[]>([]);
  const [telemetryTotal, setTelemetryTotal] = createSignal(0);
  const [telemetryLoading, setTelemetryLoading] = createSignal(false);
  // 分类总览(全量计数 + 每类聚合 + 设备画像);分类 chip 选中态('' = 全部);展开的记录 id 集合
  const [telemetrySummary, setTelemetrySummary] = createSignal<TelemetryDeviceSummary | null>(null);
  const [telemetryFilter, setTelemetryFilter] = createSignal('');
  const [expandedTel, setExpandedTel] = createSignal<Set<string>>(new Set());
  // 原始明细(分类 chips + 记录列表)默认折叠;概览在上,要翻记录才展开
  const [showDetail, setShowDetail] = createSignal(false);
  // 正在请求遥测的 deviceId，避免重复点击"拉取遥测"
  const [requestingTelemetry, setRequestingTelemetry] = createSignal<string | null>(null);
  let telemetryRequestId = 0;

  // 当前选中分类对应的聚合行('' 时为 null,头部聚合改走全量派生)
  const selectedStat = createMemo(() => {
    const f = telemetryFilter();
    return f ? telemetrySummary()?.byEventType.find((s) => s.eventType === f) ?? null : null;
  });

  const loadDevices = async () => {
    try {
      setLoading(true);
      // m027:先拉 SSE 列表(关键路径)→ 再拉平台/版本/策略聚合(非关键,失败仅 warn)。
      // 顺序非并行,避免 happy-dom 在 vi.fn mocks 下 Promise.all 的 microtask 时序
      // 引发"Tab 计数未刷新"测试 flake。
      const data = await adminApi.getClients();
      setSseLive(data.sseLive);
      setRecentlyActive(data.recentlyActive);
      try {
        const dist = await adminApi.getClientsDistribution();
        setPlatforms(dist.platforms);
        setVersionRows(dist.versions);
        setPolicies(dist.policies);
      } catch (e: any) {
        uiStore.toast.warning('设备聚合数据加载失败', e?.message);
      }
    } catch (e: any) {
      uiStore.toast.error('加载设备列表失败', e.message);
    } finally {
      setLoading(false);
    }
  };

  // m027:全部设备 tab — 后端分页查询(独立 loading,避免占用主 tab)
  const loadAllDevices = async () => {
    try {
      setAllLoading(true);
      const data = await adminApi.getClientsPaginated({
        page: allPage(),
        perPage: allPerPage(),
        q: allQ() || undefined,
        platform: allPlatform() || undefined,
      });
      setAllRows(data.data);
      setAllTotal(data.total);
    } catch (e: any) {
      uiStore.toast.error('加载设备分页失败', e.message);
    } finally {
      setAllLoading(false);
    }
  };

  // m027:CSV 导出 — 当前页(对齐设计图"导出 CSV"按钮)。
  // 实现策略:不拉全集(可能 1000+ 行),仅导当前分页结果。需要全集时增大 perPage 再点。
  const exportCsv = () => {
    const rows = allRows();
    if (rows.length === 0) {
      uiStore.toast.info('当前页无数据可导出');
      return;
    }
    const header = ['deviceId', 'platform', 'userId', 'appVersion', 'model', 'country', 'firstSeenAt', 'lastSeenAt', 'isBanned'];
    const escape = (v: unknown) => {
      const s = v == null ? '' : String(v);
      return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    };
    const lines = [
      header.join(','),
      ...rows.map((r) => header.map((k) => escape((r as any)[k])).join(',')),
    ];
    const blob = new Blob([lines.join('\n')], { type: 'text/csv;charset=utf-8;' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `devices-page-${allPage()}-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  // m027:升级策略保存
  const savePolicy = async (platform: string) => {
    const draft = policyDraft()[platform];
    const original = policies().find((p) => p.platform === platform);
    if (!draft || !original) return;
    setSavingPolicy(platform);
    try {
      await adminApi.putUpgradePolicy(platform, {
        minVersion: draft.minVersion ?? original.minVersion,
        suggestedVersion: draft.suggestedVersion ?? original.suggestedVersion,
        grayscalePct: draft.grayscalePct ?? original.grayscalePct,
        pwaSilentUpdate: draft.pwaSilentUpdate ?? original.pwaSilentUpdate,
      });
      uiStore.toast.success(`${platform} 策略已保存`);
      const dist = await adminApi.getClientsDistribution();
      setPolicies(dist.policies);
      // 清掉本平台 draft
      setPolicyDraft((d) => ({ ...d, [platform]: {} }));
    } catch (e: any) {
      uiStore.toast.error(`${platform} 策略保存失败`, e.message);
    } finally {
      setSavingPolicy(null);
    }
  };

  // D4:加载版本门控配置(失败仅 warn,不阻断设备页)
  const loadVersionGate = async () => {
    try {
      const g = await adminApi.getVersionGate();
      batch(() => {
        setVersionGate(g);
        setVgEnabled(g.enabled);
        setVgMinVersion(g.minClientVersion ?? '');
      });
    } catch (e: any) {
      uiStore.toast.warning('版本门控配置加载失败', e?.message);
    }
  };

  // D4:保存版本门控(写端点即时生效,后端刷新 strict-mode 运行时快照)
  const saveVersionGate = async () => {
    if (vgVersionInvalid()) {
      uiStore.toast.error('版本号非法', '请输入合法 semver,如 1.2.0');
      return;
    }
    setVgSaving(true);
    try {
      const g = await adminApi.setVersionGate({
        enabled: vgEnabled(),
        minClientVersion: vgMinVersion().trim() || null,
      });
      batch(() => {
        setVersionGate(g);
        setVgEnabled(g.enabled);
        setVgMinVersion(g.minClientVersion ?? '');
      });
      uiStore.toast.success('版本门控已更新', g.enabled ? `低于 ${g.effectiveMinClientVersion ?? '—'} 的客户端将被拒绝` : '门控已关闭');
    } catch (e: any) {
      uiStore.toast.error('版本门控保存失败', e?.message);
    } finally {
      setVgSaving(false);
    }
  };

  // m027:强制升级广播
  const sendBroadcastUpgrade = async () => {
    const target = broadcastUpgradeOpen();
    if (!target) return;
    setBroadcastUpgradeSending(true);
    try {
      const result = await adminApi.broadcastUpgrade(target.platform, {
        belowVersion: target.below,
        latestVersion: target.latest,
        message: broadcastUpgradeMessage() || undefined,
      });
      uiStore.toast.success(`已派发 ${target.platform} 强制升级`, `匹配 ${result.matched} 设备 · 触达 ${result.pushedConnections} 个活跃连接`);
      setBroadcastUpgradeOpen(null);
      setBroadcastUpgradeMessage('');
    } catch (e: any) {
      uiStore.toast.error('强制升级广播失败', e.message);
    } finally {
      setBroadcastUpgradeSending(false);
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
      // 一次性切换设备 + 清空旧数据 + 重置分类/展开 + 进入加载态，避免分段渲染闪烁
      batch(() => {
        setTelemetryDevice(deviceId);
        setTelemetryRecords([]);
        setTelemetryTotal(0);
        setTelemetrySummary(null);
        setTelemetryFilter('');
        setExpandedTel(new Set<string>());
        setShowDetail(false);
        setTelemetryLoading(true);
      });
      // 并行:分类总览(全量计数,系统分类)+ 首屏记录(最近 50 条)
      const [summary, data] = await Promise.all([
        adminApi.getTelemetrySummary(deviceId),
        adminApi.getTelemetry(deviceId),
      ]);
      if (requestId !== telemetryRequestId) return;
      batch(() => {
        setTelemetrySummary(summary);
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

  // 分类 chip 选中:按 event_type 重新拉记录列表(全量计数仍来自 summary,不变)
  const selectTelemetryFilter = async (eventType: string) => {
    const deviceId = telemetryDevice();
    if (!deviceId || telemetryFilter() === eventType) return;
    const requestId = ++telemetryRequestId;
    batch(() => {
      setTelemetryFilter(eventType);
      setExpandedTel(new Set<string>());
      setTelemetryLoading(true);
    });
    try {
      const data = await adminApi.getTelemetry(deviceId, eventType ? { eventType } : undefined);
      if (requestId !== telemetryRequestId) return;
      batch(() => {
        setTelemetryRecords(data.records);
        setTelemetryTotal(data.total);
      });
    } catch (e: any) {
      if (requestId !== telemetryRequestId) return;
      uiStore.toast.error('加载分类遥测失败', e.message);
    } finally {
      if (requestId === telemetryRequestId) setTelemetryLoading(false);
    }
  };

  const toggleTelExpand = (id: string) => {
    setExpandedTel((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  onMount(loadDevices);
  onMount(loadVersionGate);

  // m027:进入"全部设备"tab 或筛选变化时,拉分页。
  createEffect(() => {
    if (tab() !== 'all') return;
    // 触发 reactive 依赖 allPage / allQ / allPlatform
    allPage();
    allQ();
    allPlatform();
    void loadAllDevices();
  });

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
      key: 'appVersion',
      title: '版本',
      render: (r: SseLiveEntry) => (
        <span class="font-mono text-xs tabular-nums text-fg-muted">{r.appVersion ?? '—'}</span>
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
      key: 'country',
      title: '国家',
      render: () => (
        <span class="text-xs text-content-tertiary" title="实时连接未透出国家字段,点详情按设备 IP 解析">—</span>
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
          <Button size="xs" variant="ghost" onClick={() => setDetailDevice({
            deviceId: r.deviceId, platform: r.platform, userId: r.userId, appVersion: r.appVersion,
            isBanned: r.isBanned, dataChannels: r.dataChannels,
            connectionCount: r.connectionCount, connectedSecs: r.connectedSecs,
          })}>详情</Button>
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
      key: 'appVersion',
      title: '版本',
      render: (r: RecentlyActiveEntry) => (
        <span class="font-mono text-xs tabular-nums text-fg-muted">{r.appVersion ?? '—'}</span>
      ),
    },
    {
      key: 'userId',
      title: '用户',
      render: (r: RecentlyActiveEntry) => (
        <span class="font-mono text-xs tabular-nums">{r.userId ? truncateId(r.userId) : '-'}</span>
      ),
    },
    {
      key: 'country',
      title: '国家',
      render: () => (
        <span class="text-xs text-content-tertiary" title="近期活跃聚合未透出国家字段,点详情按设备 IP 解析">—</span>
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
          <Button size="xs" variant="ghost" onClick={() => setDetailDevice({
            deviceId: r.deviceId, platform: r.platform, userId: r.userId, appVersion: r.appVersion,
            isBanned: r.isBanned, dataChannels: r.dataChannels, lastSeenAt: r.lastSeenAt,
          })}>详情</Button>
        </div>
      ),
    },
  ];

  // m027:动态 page-desc 数字 — 对齐设计图 'X 个绑定设备 · Y 个 7 天内在线 · 三端版本分布...'
  const totalDevices = createMemo(() => platforms().reduce((a, b) => a + b.total, 0));
  const totalActive7d = createMemo(() => platforms().reduce((a, b) => a + b.active7d, 0));

  // m032:版本分布底部"低版本汇总" — 跨平台统计低于各自 minVersion 的设备数(无法支持新 AMAS 协议)
  const cmpVersion = (a: string, b: string): number => {
    const norm = (s: string) => s.replace(/^v/i, '').split(/[.+-]/).map((x) => Number(x) || 0);
    const pa = norm(a);
    const pb = norm(b);
    for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
      const d = (pa[i] ?? 0) - (pb[i] ?? 0);
      if (d !== 0) return d > 0 ? 1 : -1;
    }
    return 0;
  };
  // 取首个配了 minVersion 的平台作为强制升级目标(设计图为单按钮)
  const lowVersionTarget = createMemo(() => policies().find((p) => p.minVersion) ?? null);
  const lowVersionCount = createMemo(() => {
    let n = 0;
    for (const p of policies()) {
      if (!p.minVersion) continue;
      for (const v of versionRows()) {
        if (v.platform !== p.platform || v.version === 'unknown') continue;
        if (cmpVersion(v.version, p.minVersion) < 0) n += v.count;
      }
    }
    return n;
  });

  // m027:嵌入式广播推送 section(对齐 clients.html 设计图 compose 区块)
  const [bcTitle, setBcTitle] = createSignal('');
  const [bcMessage, setBcMessage] = createSignal('');
  const [bcPlatforms, setBcPlatforms] = createSignal<Set<string>>(new Set());
  const [bcVersionMin, setBcVersionMin] = createSignal('');
  const [bcLastActiveDays, setBcLastActiveDays] = createSignal('');
  const [bcPreviewMatched, setBcPreviewMatched] = createSignal<number | null>(null);
  const [bcPreviewTotal, setBcPreviewTotal] = createSignal<number | null>(null);
  // E1:preview 失败原因——'invalid_version'(版本号非法,可操作) / 'network'(真实网络失败) / null(正常)
  const [bcPreviewError, setBcPreviewError] = createSignal<'invalid_version' | 'network' | null>(null);
  const [bcSending, setBcSending] = createSignal(false);
  // m042/D2:投递时机(now=立即 / at=指定时间) + 指定时间值(datetime-local) + 草稿保存态
  const [bcSchedule, setBcSchedule] = createSignal<'now' | 'at'>('now');
  const [bcScheduledAt, setBcScheduledAt] = createSignal('');
  const [bcSavingDraft, setBcSavingDraft] = createSignal(false);
  // W2-2:待发定时广播队列 + 取消态
  const [scheduledList, setScheduledList] = createSignal<ScheduledBroadcast[]>([]);
  const [scheduledLoading, setScheduledLoading] = createSignal(false);
  const [cancelTarget, setCancelTarget] = createSignal<ScheduledBroadcast | null>(null);
  const [canceling, setCanceling] = createSignal(false);

  const loadScheduled = async () => {
    setScheduledLoading(true);
    try {
      setScheduledList((await adminApi.listScheduledBroadcasts()).items);
    } catch { /* 队列拿不到不打扰 composer */ } finally {
      setScheduledLoading(false);
    }
  };
  const audienceSummary = (s: ScheduledBroadcast): string => {
    const parts: string[] = [];
    if (s.platforms.length) parts.push(s.platforms.join('/'));
    if (s.versionMin) parts.push(`≥${s.versionMin}`);
    if (s.lastActiveDays != null) parts.push(`${s.lastActiveDays}天活跃`);
    if (s.userIds.length) parts.push(`${s.userIds.length}个指定用户`);
    return parts.length ? parts.join(' · ') : '全员';
  };
  const confirmCancelScheduled = async () => {
    const t = cancelTarget();
    if (!t) return;
    setCanceling(true);
    try {
      await adminApi.cancelScheduledBroadcast(t.id);
      uiStore.toast.success('已取消该排程');
      setCancelTarget(null);
      await loadScheduled();
    } catch (e: any) {
      // 409 = 已被 worker 到点下发,无法取消
      if (e instanceof ApiError && e.code === 'ALREADY_DISPATCHED') {
        uiStore.toast.error('无法取消', '该排程已到点下发');
        setCancelTarget(null);
        await loadScheduled();
      } else {
        uiStore.toast.error('取消失败', e?.message);
      }
    } finally {
      setCanceling(false);
    }
  };

  // E1:前端 semver 预校验,与后端 `semver::Version::parse(v.trim_start_matches('v'))` 行为对齐——
  // 去前导 v 后须为合法语义化版本(major.minor.patch + 可选 -prerelease / +build)。把 INVALID_VERSION_MIN 拦在请求前。
  const isValidVersionMin = (raw: string): boolean => {
    // 后端为 `trim_start_matches('v')`(区分大小写、去连续小写 v)后 semver::Version::parse,这里同样仅去小写前导 v
    const v = raw.trim().replace(/^v+/, '');
    if (!v) return false;
    return /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(v);
  };

  const buildBcAudience = () => {
    const platforms = Array.from(bcPlatforms());
    const versionMin = bcVersionMin().trim();
    const daysStr = bcLastActiveDays().trim();
    if (platforms.length === 0 && !versionMin && !daysStr) return undefined;
    return {
      platforms: platforms.length > 0 ? platforms : undefined,
      versionMin: versionMin || undefined,
      lastActiveDays: daysStr ? Number(daysStr) || undefined : undefined,
    };
  };

  const toggleBcPlatform = (p: string) => {
    setBcPlatforms((prev) => {
      const next = new Set(prev);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  };

  // m027:预估命中 — debounce 500ms 后查 /preview;受众变化或 mount 都触发
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    bcPlatforms();
    bcVersionMin();
    bcLastActiveDays();
    clearTimeout(previewTimer);
    // E1:版本号填了但非法时不发请求,直接标为可操作错误(否则后端必回 400,白白吞掉)
    const vRaw = bcVersionMin().trim();
    if (vRaw && !isValidVersionMin(vRaw)) {
      setBcPreviewMatched(null);
      setBcPreviewTotal(null);
      setBcPreviewError('invalid_version');
      return;
    }
    previewTimer = setTimeout(async () => {
      try {
        const result = await adminApi.broadcastPreview({ audience: buildBcAudience() });
        setBcPreviewMatched(result.matched);
        setBcPreviewTotal(result.total);
        setBcPreviewError(null);
      } catch (e) {
        // E1:区分「非法版本号」(可操作)与真实网络失败
        setBcPreviewMatched(null);
        setBcPreviewTotal(null);
        setBcPreviewError(e instanceof ApiError && e.code === 'INVALID_VERSION_MIN' ? 'invalid_version' : 'network');
      }
    }, 500);
  });

  // m042/D2:把 datetime-local 值(本地时区,无 offset)转为后端要的 RFC3339。
  const scheduledAtRfc3339 = (): string | undefined => {
    if (bcSchedule() !== 'at') return undefined;
    const raw = bcScheduledAt().trim();
    if (!raw) return undefined;
    const d = new Date(raw); // datetime-local 按本地时区解析
    if (Number.isNaN(d.getTime())) return undefined;
    return d.toISOString();
  };

  const sendBc = async (preview: boolean) => {
    if (!bcTitle().trim() || !bcMessage().trim()) {
      uiStore.toast.warning('请填写标题和正文');
      return;
    }
    // 指定时间模式必须填且为未来时间
    if (!preview && bcSchedule() === 'at') {
      const at = scheduledAtRfc3339();
      if (!at) {
        uiStore.toast.warning('请选择投递时间');
        return;
      }
      if (new Date(at).getTime() <= Date.now()) {
        uiStore.toast.warning('投递时间需晚于当前时间');
        return;
      }
    }
    // E1:正式发送带受众,版本号填了但非法时前端拦截,把 INVALID_VERSION_MIN 挡在请求前
    if (!preview) {
      const vRaw = bcVersionMin().trim();
      if (vRaw && !isValidVersionMin(vRaw)) {
        uiStore.toast.warning('受众版本号非法', '需为合法语义化版本(如 v1.2.3),已停止发送');
        return;
      }
    }
    setBcSending(true);
    try {
      if (preview) {
        // 预演 = 只发给当前 admin。后端没有 self user_id 透出,这里复用 audience userIds 留空但
        // 加 title 前缀 "[预演]" 让 admin 端能识别。前端发送时 audience platforms=[admin 自身的 platform]
        // 但因为 admin 不是 client 不会命中,降级方案直接走全员发送 + 标题前缀。预演恒立即。
        await adminApi.broadcast({ title: `[预演] ${bcTitle()}`, message: bcMessage() });
        uiStore.toast.success('预演已发送(标题前缀 [预演])');
      } else {
        const result = await adminApi.broadcast({
          title: bcTitle(),
          message: bcMessage(),
          audience: buildBcAudience(),
          scheduledAt: scheduledAtRfc3339(),
        });
        if (result.scheduled) {
          uiStore.toast.success(`已排程,将于 ${new Date(result.scheduledAt!).toLocaleString()} 自动下发`);
          void loadScheduled(); // W2-2:刷新待发队列
        } else {
          uiStore.toast.success(`已发送给 ${result.sent} 位用户`);
        }
        setBcTitle('');
        setBcMessage('');
        setBcSchedule('now');
        setBcScheduledAt('');
      }
    } catch (e: any) {
      // E1:广播受众过滤 400 错误码 → 具体可操作中文提示;其余落通用「发送失败」
      if (e instanceof ApiError && e.code === 'INVALID_VERSION_MIN') {
        uiStore.toast.error('受众版本号非法', '请填写合法语义化版本(如 v1.2.3)后重试');
      } else if (e instanceof ApiError && e.code === 'EMPTY_AUDIENCE') {
        uiStore.toast.error('受众过滤后 0 人', '当前条件未匹配到任何设备,广播未发送,请放宽受众条件');
      } else {
        uiStore.toast.error('发送失败', e?.message);
      }
    } finally {
      setBcSending(false);
    }
  };

  // m042/D2:保存推送草稿(全局单份,覆盖)。
  const saveDraft = async () => {
    if (!bcTitle().trim() && !bcMessage().trim()) {
      uiStore.toast.warning('草稿至少需要标题或正文');
      return;
    }
    setBcSavingDraft(true);
    try {
      const aud = buildBcAudience();
      await adminApi.savePushDraft({
        title: bcTitle(),
        message: bcMessage(),
        platforms: aud?.platforms,
        versionMin: aud?.versionMin,
        lastActiveDays: aud?.lastActiveDays,
      });
      uiStore.toast.success('草稿已保存');
    } catch (e: any) {
      uiStore.toast.error('保存草稿失败', e?.message);
    } finally {
      setBcSavingDraft(false);
    }
  };

  // m042/D2:进入页面时回填上次保存的草稿(有则填,无则静默)。
  const loadDraft = async () => {
    try {
      const { draft } = await adminApi.getPushDraft();
      if (!draft) return;
      batch(() => {
        if (draft.title) setBcTitle(draft.title);
        if (draft.message) setBcMessage(draft.message);
        if (draft.platforms?.length) setBcPlatforms(new Set(draft.platforms));
        if (draft.versionMin) setBcVersionMin(draft.versionMin);
        if (draft.lastActiveDays != null) setBcLastActiveDays(String(draft.lastActiveDays));
      });
    } catch {
      // 草稿回填失败不阻塞页面
    }
  };
  onMount(loadDraft);
  onMount(loadScheduled);

  const audienceChips = createMemo(() => {
    const chips: { label: string; remove: () => void }[] = [];
    const ps = Array.from(bcPlatforms());
    if (ps.length > 0) {
      chips.push({ label: `platform = ${ps.join(', ')}`, remove: () => setBcPlatforms(new Set()) });
    }
    if (bcVersionMin().trim()) {
      chips.push({ label: `version ≥ ${bcVersionMin()}`, remove: () => setBcVersionMin('') });
    }
    if (bcLastActiveDays().trim()) {
      chips.push({ label: `last_active ≤ ${bcLastActiveDays()}d`, remove: () => setBcLastActiveDays('') });
    }
    return chips;
  });

  return (
    <div class="space-y-6">
      {/* m027:轻量 page-header(对齐设计图 clients.html,不用 HeroCard 的 hero/eyebrow/meta) */}
      <div class="flex items-start justify-between gap-4 flex-wrap">
        <div class="min-w-0 flex-1">
          <h1 class="text-[28px] font-bold leading-tight tracking-[-0.02em]">设备管理</h1>
          <p class="mt-1.5 text-sm text-content-secondary leading-relaxed">
            <span class="tabular-nums">{totalDevices().toLocaleString()}</span> 个绑定设备 ·
            <span class="tabular-nums"> {totalActive7d().toLocaleString()}</span> 个 7 天内在线 ·
            三端版本分布 + 强制升级 + 广播推送。所有推送写入 audit log,可按平台 / 版本 / 单用户精准投递。
          </p>
        </div>
        <div class="flex gap-2 shrink-0">
          <Button size="sm" variant="outline" onClick={loadDevices} disabled={loading()} aria-label="刷新设备列表">
            <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
            刷新
          </Button>
          <Button
            size="sm"
            variant="warning"
            disabled={policies().every((p) => !p.minVersion)}
            onClick={() => {
              const target = policies().find((p) => p.minVersion);
              if (target) {
                setBroadcastUpgradeOpen({
                  platform: target.platform,
                  below: target.minVersion!,
                  latest: target.suggestedVersion || target.minVersion!,
                });
              }
            }}
          >
            <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 11-9-9c2.52 0 4.93 1 6.74 2.74L21 8M21 3v5h-5" />
            </svg>
            全部强制升级
          </Button>
          <Button size="sm" variant="primary" onClick={() => (window.location.href = '/admin/broadcast')}>
            <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z" />
            </svg>
            新建推送
          </Button>
        </div>
      </div>

      {/* m027:平台 hero 3 卡(Web/iOS/Android × icon + total/share/active7d/月环比)。
          固定渲染 3 平台,无数据时显示 0 设备占位,对齐设计图 clients.html .platform-card */}
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <For each={['web', 'ios', 'android'] as const}>
          {(name) => {
            const agg = createMemo(() => platforms().find((p) => p.platform === name));
            const total = createMemo(() => platforms().reduce((a, b) => a + b.total, 0) || 1);
            const share = () => (agg() ? ((agg()!.total / total()) * 100) : 0);
            const pct = () => agg()?.monthOverMonthPct ?? 0;
            const pctClass = () => (pct() >= 0 ? 'text-success-strong' : 'text-error-strong');
            const pctArrow = () => (pct() >= 0 ? '▲' : '▼');
            const meta: Record<typeof name, {
              name: string;
              sub: string;
              bg: string;
              iconBg: string;
              icon: () => any;
            }> = {
              web: {
                name: 'Web 浏览器',
                sub: 'PWA · Chrome / Safari / Firefox',
                bg: 'linear-gradient(135deg, var(--accent-light) 0%, var(--surface-elevated) 65%)',
                iconBg: 'var(--accent)',
                icon: () => (
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4">
                    <circle cx="12" cy="12" r="10" />
                    <line x1="2" y1="12" x2="22" y2="12" />
                    <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                  </svg>
                ),
              },
              ios: {
                name: 'iOS 原生',
                sub: 'iPhone / iPad · App Store',
                bg: 'linear-gradient(135deg, var(--surface-tertiary) 0%, var(--surface-elevated) 65%)',
                iconBg: 'oklch(20% 0.012 250)',
                icon: () => (
                  <svg viewBox="0 0 24 24" fill="currentColor" class="w-4 h-4">
                    <path d="M17.05 20.28c-.98.95-2.05.8-3.08.35-1.09-.46-2.09-.48-3.24 0-1.44.62-2.2.44-3.06-.35C2.79 15.25 3.51 7.59 9.05 7.31c1.35.07 2.29.74 3.08.8 1.18-.24 2.31-.93 3.57-.84 1.51.12 2.65.72 3.4 1.8-3.12 1.87-2.38 5.98.48 7.13-.57 1.5-1.31 2.99-2.54 4.09zM12.03 7.25c-.15-2.23 1.66-4.07 3.74-4.25.29 2.58-2.34 4.5-3.74 4.25z" />
                  </svg>
                ),
              },
              android: {
                name: 'Android 原生',
                sub: 'Google Play / 应用宝 / APK',
                bg: 'linear-gradient(135deg, var(--success-light) 0%, var(--surface-elevated) 65%)',
                iconBg: 'oklch(58% 0.16 162)',
                icon: () => (
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4">
                    <path d="M5 16V8a4 4 0 0 1 4-4h6a4 4 0 0 1 4 4v8M9 20v-3M15 20v-3" />
                    <circle cx="9" cy="11" r="1" fill="currentColor" />
                    <circle cx="15" cy="11" r="1" fill="currentColor" />
                  </svg>
                ),
              },
            };
            return (
              <div class="rounded-2xl p-[18px] shadow-elevation-1 relative overflow-hidden" style={{ background: meta[name].bg }}>
                <div class="flex items-center gap-2.5">
                  <div
                    class="w-8 h-8 rounded-[9px] grid place-items-center text-white shrink-0"
                    style={{ background: meta[name].iconBg }}
                  >
                    {meta[name].icon()}
                  </div>
                  <div class="min-w-0">
                    <div class="text-sm font-semibold leading-tight">{meta[name].name}</div>
                    <div class="text-[11px] text-content-tertiary truncate">{meta[name].sub}</div>
                  </div>
                </div>
                <div class="mt-3 text-[28px] font-bold tabular-nums leading-none tracking-[-0.025em]">
                  {(agg()?.total ?? 0).toLocaleString()}
                  <span class="text-xs text-content-tertiary font-medium ml-1.5 align-baseline">设备 · {share().toFixed(1)}%</span>
                </div>
                <div class="mt-2.5 flex gap-3 text-[11.5px] text-content-secondary font-mono tabular-nums">
                  <span>
                    <span class={pctClass()}>{pctArrow()} {Math.abs(pct()).toFixed(0)}%</span> 比上月
                  </span>
                  <span>{(agg()?.active7d ?? 0).toLocaleString()} 在线 (7d)</span>
                </div>
              </div>
            );
          }}
        </For>
      </div>

      {/* m027:版本分布柱(三平台 × N 档版本)+ 升级策略面板(右侧)。
          外层去 Show,空数据时显示骨架对齐设计图视觉;policies seed 至少 3 行 */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-4">
          <Card class="lg:col-span-7" padding="md">
            <div class="flex items-center justify-between mb-3">
              <h3 class="text-sm font-semibold text-content">版本分布 · 各平台</h3>
              <div class="flex gap-1" role="tablist" aria-label="版本分布平台过滤">
                <For each={['all', 'web', 'ios', 'android'] as const}>
                  {(k) => (
                    <Button
                      size="xs"
                      variant={versionFilter() === k ? 'primary' : 'ghost'}
                      onClick={() => setVersionFilter(k)}
                    >
                      {k === 'all' ? '全部' : k.toUpperCase()}
                    </Button>
                  )}
                </For>
              </div>
            </div>
            <For each={(['web', 'ios', 'android'] as const).filter((p) => versionFilter() === 'all' || versionFilter() === p)}>
              {(plt) => {
                const rows = createMemo(() => versionRows().filter((v) => v.platform === plt));
                const max = createMemo(() => rows().reduce((m, r) => Math.max(m, r.count), 0) || 1);
                const total = createMemo(() => rows().reduce((a, b) => a + b.count, 0));
                return (
                  <div class="mt-3 first:mt-0">
                    <h4 class="text-[11px] font-semibold uppercase tracking-wider text-content-secondary mb-2">
                      {plt.toUpperCase()} ({total().toLocaleString()} 设备)
                    </h4>
                    <Show
                      when={rows().length > 0}
                      fallback={<div class="text-xs text-content-tertiary py-1">— 暂无数据 · 等待客户端上报 x-app-version 头</div>}
                    >
                      <For each={rows().slice(0, 6)}>
                        {(r, idx) => (
                          <div class="grid grid-cols-[110px_1fr_60px] items-center gap-2 py-1 text-[12.5px]">
                            <span class="font-mono tabular-nums">
                              <span class={idx() === 0 ? 'text-success-strong font-semibold' : ''}>{r.version === 'unknown' ? '未上报' : r.version}</span>
                            </span>
                            <div class="h-2 rounded-full bg-surface-secondary overflow-hidden">
                              <div
                                class="h-full rounded-full transition-[width] duration-base ease-out-expo"
                                style={{
                                  width: `${((r.count / max()) * 100).toFixed(1)}%`,
                                  background: idx() === 0 ? 'var(--accent)' : idx() < 3 ? 'var(--warning)' : 'var(--error)',
                                }}
                              />
                            </div>
                            <span class="font-mono tabular-nums text-right text-content-secondary">{r.count.toLocaleString()}</span>
                          </div>
                        )}
                      </For>
                    </Show>
                  </div>
                );
              }}
            </For>
            {/* m032:低版本汇总行 + inline 强制升级(对齐设计图 .vd 底部 row spread) */}
            <Show when={lowVersionTarget()}>
              <div class="border-t border-border-hairline mt-3 pt-3 flex items-center justify-between gap-3 flex-wrap">
                <span class="text-[12.5px] text-content-secondary">
                  <span class="tabular-nums font-semibold">{lowVersionCount().toLocaleString()}</span> 个设备低于 {lowVersionTarget()!.minVersion},无法支持新 AMAS 协议
                </span>
                <button
                  type="button"
                  class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-warning-light text-warning-strong text-[12.5px] font-medium hover:brightness-95 transition cursor-pointer"
                  onClick={() => {
                    const t = lowVersionTarget()!;
                    setBroadcastUpgradeOpen({
                      platform: t.platform,
                      below: t.minVersion!,
                      latest: t.suggestedVersion || t.minVersion!,
                    });
                  }}
                >
                  <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 2L4 6v6c0 5.5 3.5 10 8 10s8-4.5 8-10V6l-8-4z" />
                  </svg>
                  对老版本强制升级
                </button>
              </div>
            </Show>
          </Card>

          {/* m027:强制升级策略面板(右 5/12) */}
          <Card class="lg:col-span-5" padding="md">
            <div class="flex items-center justify-between mb-3">
              <h3 class="text-sm font-semibold text-content">强制升级策略</h3>
              <span class="text-xs text-content-tertiary tabular-nums">每平台独立</span>
            </div>
            <For each={policies()}>
              {(p) => {
                const draftOf = (k: keyof ClientUpgradePolicy) => policyDraft()[p.platform]?.[k];
                const setDraft = (k: keyof ClientUpgradePolicy, v: any) =>
                  setPolicyDraft((d) => ({ ...d, [p.platform]: { ...d[p.platform], [k]: v } }));
                const dirty = () => {
                  const d = policyDraft()[p.platform];
                  return d && Object.keys(d).length > 0;
                };
                return (
                  <div class="border-b border-border-hairline last:border-b-0 py-3">
                    <div class="flex items-center justify-between mb-2">
                      <Badge size="sm" variant="default">{p.platform.toUpperCase()}</Badge>
                      <Show when={dirty()}>
                        <Button
                          size="xs"
                          variant="primary"
                          disabled={savingPolicy() === p.platform}
                          onClick={() => savePolicy(p.platform)}
                        >
                          {savingPolicy() === p.platform ? '保存中…' : '保存'}
                        </Button>
                      </Show>
                    </div>
                    {/* 最低支持版本 行 — 对齐设计图 row spread */}
                    <div class="flex items-center justify-between py-1.5">
                      <div class="min-w-0">
                        <div class="text-[13px] font-semibold">最低支持版本</div>
                        <div class="text-[11px] text-content-tertiary mt-0.5">低于此版本启动时弹强制升级窗口</div>
                      </div>
                      <Input
                        size="sm"
                        placeholder="v0.6.5"
                        value={(draftOf('minVersion') ?? p.minVersion ?? '') as string}
                        onInput={(e) => setDraft('minVersion', e.currentTarget.value || null)}
                        class="!w-[110px] font-mono"
                      />
                    </div>
                    <div class="flex items-center justify-between py-1.5 border-t border-border-hairline">
                      <div class="min-w-0">
                        <div class="text-[13px] font-semibold">建议升级</div>
                        <div class="text-[11px] text-content-tertiary mt-0.5">非阻断式 · 顶部黄色横幅</div>
                      </div>
                      <Input
                        size="sm"
                        placeholder="v0.7.0"
                        value={(draftOf('suggestedVersion') ?? p.suggestedVersion ?? '') as string}
                        onInput={(e) => setDraft('suggestedVersion', e.currentTarget.value || null)}
                        class="!w-[110px] font-mono"
                      />
                    </div>
                    <div class="flex items-center justify-between py-1.5 border-t border-border-hairline">
                      <div class="min-w-0">
                        <div class="text-[13px] font-semibold">灰度发布 %</div>
                        <div class="text-[11px] text-content-tertiary mt-0.5">向 N% 用户先放出新版浮窗</div>
                      </div>
                      <Input
                        size="sm"
                        type="number"
                        min="0"
                        max="100"
                        value={String(draftOf('grayscalePct') ?? p.grayscalePct)}
                        onInput={(e) => setDraft('grayscalePct', Number(e.currentTarget.value))}
                        class="!w-[70px] font-mono"
                      />
                    </div>
                    <Show when={p.platform === 'web'}>
                      <div class="flex items-center justify-between py-1.5 border-t border-border-hairline">
                        <div class="min-w-0">
                          <div class="text-[13px] font-semibold">PWA 后台静默更新</div>
                          <div class="text-[11px] text-content-tertiary mt-0.5">下次访问自动应用 service worker</div>
                        </div>
                        <Switch
                          checked={(draftOf('pwaSilentUpdate') ?? p.pwaSilentUpdate) as boolean}
                          onChange={(v) => setDraft('pwaSilentUpdate', v)}
                        />
                      </div>
                    </Show>
                    <Show when={p.minVersion}>
                      <button
                        type="button"
                        class="mt-3 w-full flex items-center justify-center gap-1.5 px-3 py-2 rounded-md bg-warning-light text-warning-strong text-[13px] font-medium hover:brightness-95 transition cursor-pointer"
                        onClick={() => {
                          setBroadcastUpgradeOpen({
                            platform: p.platform,
                            below: p.minVersion!,
                            latest: p.suggestedVersion || p.minVersion!,
                          });
                        }}
                      >
                        <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                          <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a9 9 0 11-9-9c2.52 0 4.93 1 6.74 2.74L21 8M21 3v5h-5" />
                        </svg>
                        立即推送 {p.suggestedVersion || p.minVersion} 给老版本设备
                      </button>
                    </Show>
                  </div>
                );
              }}
            </For>
            <Show when={policies().length === 0}>
              <div class="text-xs text-content-tertiary py-2 text-center">
                — policies 未加载 · 请重启后端让 m024_client_extras migration 跑过
              </div>
            </Show>
          </Card>
        </div>

      {/* D4:客户端最低版本门控 — 全局 strict-mode 运行时切流(独立于上面的每平台升级策略)。
          开启后服务端直接对低于阈值的客户端返回 CLIENT_OUTDATED,即时生效。 */}
      <div>
        <div class="flex items-baseline gap-3 mb-3">
          <h2 class="text-sm font-semibold">版本门控</h2>
          <span class="text-xs text-content-tertiary">全局服务端切流 · 低于阈值的客户端请求直接被拒(CLIENT_OUTDATED) · 即时生效</span>
        </div>
        <Card padding="md">
          <div class="flex flex-col gap-3">
            <div class="flex items-center justify-between py-1.5">
              <div class="min-w-0 pr-4">
                <div class="text-[13px] font-semibold">启用版本门控</div>
                <div class="text-[11px] text-content-tertiary mt-0.5">
                  开启后,所有 /api/* 客户端请求若 UA 版本低于阈值,服务端返回 400 CLIENT_OUTDATED。
                  与"强制升级策略"(客户端启动弹窗,可绕过)不同,这是硬性服务端拒绝。
                </div>
              </div>
              <Switch checked={vgEnabled()} onChange={setVgEnabled} />
            </div>
            <div class="flex items-center justify-between py-1.5 border-t border-border-hairline">
              <div class="min-w-0 pr-4">
                <div class="text-[13px] font-semibold">最低客户端版本</div>
                <div class="text-[11px] text-content-tertiary mt-0.5">
                  semver(如 1.2.0)。留空则回落环境变量 MIN_CLIENT_VERSION
                  <Show when={versionGate()?.envMinClientVersion}>
                    {(env) => <span class="font-mono">（当前 env: {env()}）</span>}
                  </Show>
                  。
                </div>
              </div>
              <div class="flex flex-col items-end gap-1">
                <Input
                  size="sm"
                  placeholder="1.2.0"
                  value={vgMinVersion()}
                  onInput={(e) => setVgMinVersion(e.currentTarget.value)}
                  class="!w-[130px] font-mono"
                />
                <Show when={vgVersionInvalid()}>
                  <span class="text-[11px] text-error">非法 semver,应为 x.y.z</span>
                </Show>
              </div>
            </div>
            <div class="flex items-center justify-between pt-2 border-t border-border-hairline">
              <div class="text-[11px] text-content-tertiary">
                实际生效阈值:
                <span class="font-mono text-content">{versionGate()?.effectiveMinClientVersion ?? '—'}</span>
                <Show when={!versionGate()?.enabled && (versionGate()?.effectiveMinClientVersion)}>
                  <span class="ml-1">(门控未启用,阈值不生效)</span>
                </Show>
              </div>
              <Button
                size="sm"
                variant="primary"
                disabled={vgSaving() || !vgDirty() || vgVersionInvalid()}
                onClick={saveVersionGate}
              >
                {vgSaving() ? '保存中…' : '保存并即时生效'}
              </Button>
            </div>
          </div>
        </Card>
      </div>

      {/* m027:嵌入式广播推送 section — 对齐 clients.html .compose 区块。
          含真功能(标题/正文/chip 受众/预估命中/发送/预演)+ 占位控件(投递时机/渠道/优先级) */}
      <div>
        <div class="flex items-baseline gap-3 mb-3">
          <h2 class="text-sm font-semibold">广播推送</h2>
          <span class="text-xs text-content-tertiary">公告 / 词库更新 / 维护通知 · 按平台 / 版本 / 单用户精准投递</span>
        </div>
        <Card padding="lg">
          <div class="grid grid-cols-1 lg:grid-cols-[1fr_360px] gap-6">
            {/* 左:表单 */}
            <div class="space-y-3">
              <Input
                label="标题"
                size="sm"
                placeholder="例如: 新词库已上架: IELTS 学术核心 4521 词"
                value={bcTitle()}
                onInput={(e) => setBcTitle(e.currentTarget.value)}
                maxlength={200}
              />
              <div>
                <label class="text-xs text-content-tertiary block mb-1">正文</label>
                <textarea
                  class="w-full rounded-md border border-border-hairline bg-surface px-3 py-2 text-sm leading-relaxed focus:border-accent focus:outline-none resize-y"
                  rows={3}
                  placeholder="支持 Markdown 链接 [text](url),最大 280 字"
                  value={bcMessage()}
                  onInput={(e) => setBcMessage(e.currentTarget.value)}
                  maxlength={280}
                />
                <div class="text-[11px] text-content-tertiary mt-0.5">支持 Markdown 链接 [text](url) · 最大 280 字 · 当前 {bcMessage().length}/280</div>
              </div>

              {/* 受众过滤 chip + 添加 */}
              <div>
                <label class="text-xs text-content-tertiary block mb-1.5">受众过滤</label>
                <div class="flex flex-wrap gap-1.5 items-center">
                  <For each={audienceChips()}>
                    {(chip) => (
                      <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-surface-secondary text-xs">
                        {chip.label}
                        <button
                          type="button"
                          aria-label={`移除 ${chip.label}`}
                          class="w-3.5 h-3.5 rounded-full grid place-items-center bg-content/15 text-content-tertiary text-[10px] cursor-pointer hover:bg-content/25"
                          onClick={chip.remove}
                        >×</button>
                      </span>
                    )}
                  </For>
                  {/* 添加平台 */}
                  <For each={(['web', 'ios', 'android'] as const).filter((p) => !bcPlatforms().has(p))}>
                    {(p) => (
                      <button
                        type="button"
                        class="px-2 py-0.5 rounded-full text-xs border border-border-hairline text-content-secondary hover:bg-surface-secondary cursor-pointer"
                        onClick={() => toggleBcPlatform(p)}
                      >+ {p}</button>
                    )}
                  </For>
                  {/* 添加 version / lastActive */}
                  <Show when={!bcVersionMin().trim()}>
                    <button
                      type="button"
                      class="px-2 py-0.5 rounded-full text-xs border border-border-hairline text-content-secondary hover:bg-surface-secondary cursor-pointer"
                      onClick={() => setBcVersionMin('v0.7.0')}
                    >+ 版本</button>
                  </Show>
                  <Show when={!bcLastActiveDays().trim()}>
                    <button
                      type="button"
                      class="px-2 py-0.5 rounded-full text-xs border border-border-hairline text-content-secondary hover:bg-surface-secondary cursor-pointer"
                      onClick={() => setBcLastActiveDays('7')}
                    >+ 最近活跃</button>
                  </Show>
                </div>
                {/* 受众条件 inline 编辑 (version / lastActive) */}
                <Show when={bcVersionMin().trim() || bcLastActiveDays().trim()}>
                  <div class="grid grid-cols-2 gap-2 mt-2">
                    <Show when={bcVersionMin().trim()}>
                      <div class="flex flex-col gap-0.5">
                        <Input
                          size="sm"
                          placeholder="版本下限,如 v0.7.0"
                          value={bcVersionMin()}
                          onInput={(e) => setBcVersionMin(e.currentTarget.value)}
                          aria-invalid={!isValidVersionMin(bcVersionMin())}
                        />
                        {/* E1:semver 内联校验提示 */}
                        <Show when={!isValidVersionMin(bcVersionMin())}>
                          <span class="text-[11px] text-error">版本号非法,需为语义化版本(如 v1.2.3)</span>
                        </Show>
                      </div>
                    </Show>
                    <Show when={bcLastActiveDays().trim()}>
                      <Input
                        size="sm"
                        type="number"
                        min="1"
                        placeholder="最近活跃天数"
                        value={bcLastActiveDays()}
                        onInput={(e) => setBcLastActiveDays(e.currentTarget.value)}
                      />
                    </Show>
                  </div>
                </Show>
                {/* E1:预估命中 — 区分非法版本号(可操作)与真实网络失败,不再静默显「—」 */}
                <Show
                  when={bcPreviewError() === null}
                  fallback={
                    <div class="text-[11px] text-error mt-1.5">
                      {bcPreviewError() === 'invalid_version'
                        ? '版本号非法,无法预估命中 — 请填写语义化版本(如 v1.2.3)'
                        : '预估命中加载失败,请检查网络后重试'}
                    </div>
                  }
                >
                  <div class="text-[11px] text-content-tertiary mt-1.5">
                    预估命中 <strong class="text-accent tabular-nums">{bcPreviewMatched()?.toLocaleString() ?? '—'}</strong>
                    {' '}/ {bcPreviewTotal()?.toLocaleString() ?? '—'} 用户
                  </div>
                </Show>
              </div>

              {/* 投递时机(可用) / 渠道(待 provider,保持 disabled) / 优先级(待协议扩展,保持 disabled) */}
              <div class="grid grid-cols-3 gap-3">
                <label class="flex flex-col gap-0.5">
                  <span class="text-xs text-content-tertiary">投递时机</span>
                  <select
                    aria-label="投递时机"
                    class="text-sm rounded-md border border-border-hairline bg-surface px-2 py-1.5 text-content-secondary"
                    value={bcSchedule()}
                    onChange={(e) => setBcSchedule(e.currentTarget.value as 'now' | 'at')}
                  >
                    <option value="now">立即</option>
                    <option value="at">指定时间</option>
                  </select>
                </label>
                <div class="flex flex-col gap-0.5">
                  <span class="text-xs text-content-tertiary">渠道</span>
                  {/* APNs/FCM 需外部 provider,保持 disabled(D2 陷阱:勿动) */}
                  <div class="flex gap-2 text-xs mt-1 text-content-tertiary" title="APNs / FCM 需接入外部 push provider 后启用">
                    <label class="flex gap-1 items-center"><input type="checkbox" disabled checked /> Web Push</label>
                    <label class="flex gap-1 items-center"><input type="checkbox" disabled /> APNs</label>
                    <label class="flex gap-1 items-center"><input type="checkbox" disabled /> FCM</label>
                  </div>
                </div>
                <label class="flex flex-col gap-0.5">
                  <span class="text-xs text-content-tertiary">优先级</span>
                  <select
                    class="text-sm rounded-md border border-border-hairline bg-surface px-2 py-1.5 text-content-tertiary cursor-not-allowed"
                    disabled
                    title="接入通知协议扩展后启用"
                  >
                    <option>普通</option>
                  </select>
                </label>
              </div>

              {/* 指定时间模式:datetime-local 输入 */}
              <Show when={bcSchedule() === 'at'}>
                <label class="flex flex-col gap-0.5">
                  <span class="text-xs text-content-tertiary">投递时间</span>
                  <Input
                    aria-label="投递时间"
                    size="sm"
                    type="datetime-local"
                    value={bcScheduledAt()}
                    onInput={(e) => setBcScheduledAt(e.currentTarget.value)}
                  />
                </label>
              </Show>

              <div class="flex gap-2 pt-1 flex-wrap">
                <Button size="sm" variant="outline" disabled={bcSavingDraft()} onClick={saveDraft}>
                  {bcSavingDraft() ? '保存中…' : '保存草稿'}
                </Button>
                <Button size="sm" variant="outline" disabled={bcSending() || !bcTitle().trim() || !bcMessage().trim()} onClick={() => sendBc(true)}>
                  先发给我(预演)
                </Button>
                <Button
                  size="sm"
                  variant="primary"
                  disabled={bcSending() || !bcTitle().trim() || !bcMessage().trim()}
                  onClick={() => sendBc(false)}
                >
                  <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z" />
                  </svg>
                  {bcSchedule() === 'at' ? '排程' : '推送'}给 {bcPreviewMatched()?.toLocaleString() ?? '—'} 用户
                </Button>
              </div>
            </div>

            {/* 右:iPhone 锁屏预览 */}
            <div
              class="rounded-2xl p-4 relative overflow-hidden shadow-elevation-2 hidden lg:block"
              style={{
                background: 'linear-gradient(180deg, oklch(22% 0.018 258), oklch(15% 0.012 258))',
              }}
            >
              {/* notch */}
              <div class="w-[88px] h-[22px] mx-auto -mb-3 rounded-full relative z-10" style={{ background: 'oklch(8% 0 0)' }} />
              {/* status bar */}
              <div class="flex justify-between items-center text-white/90 text-[13px] font-semibold px-2 pb-3">
                <span>22:47</span>
                <span class="flex gap-1 items-center">
                  <svg width="14" height="10" viewBox="0 0 14 10" fill="currentColor"><path d="M0 7h2v3H0zm4-2h2v5H4zm4-2h2v7H8zm4-2h2v9h-2z" /></svg>
                  <svg width="24" height="11" viewBox="0 0 24 11" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="0" y="0" width="22" height="11" rx="2.5" /><rect x="2" y="2" width="14" height="7" rx="1" fill="currentColor" /><rect x="22.5" y="3" width="1.5" height="5" rx="0.5" fill="currentColor" /></svg>
                </span>
              </div>
              {/* lock time */}
              <div class="text-white text-center text-[56px] font-light leading-none mb-1 tracking-[-0.02em]">22<span class="opacity-50">:</span>47</div>
              <div class="text-white/60 text-center text-[13px] font-medium mb-6">星期二 · 5 月 26 日</div>
              {/* notification card */}
              <div
                class="rounded-xl p-3 text-white border"
                style={{
                  background: 'rgba(255,255,255,0.08)',
                  'backdrop-filter': 'blur(20px)',
                  '-webkit-backdrop-filter': 'blur(20px)',
                  'border-color': 'rgba(255,255,255,0.1)',
                }}
              >
                <div class="flex items-center gap-2 text-[11.5px] text-white/60 mb-1">
                  <span
                    class="w-4 h-4 rounded grid place-items-center text-[9px] font-bold text-white"
                    style={{ background: 'linear-gradient(135deg, var(--accent), oklch(54% 0.22 290))' }}
                  >W</span>
                  <span>WordForge · 现在</span>
                </div>
                <div class="text-[14px] font-semibold leading-snug mb-1">{bcTitle() || '(标题预览)'}</div>
                <div class="text-[13px] leading-snug text-white/85 line-clamp-3">{bcMessage() || '(正文预览)'}</div>
              </div>
              <div
                class="rounded-xl p-3 text-white border mt-2 opacity-50"
                style={{
                  background: 'rgba(255,255,255,0.08)',
                  'backdrop-filter': 'blur(20px)',
                  '-webkit-backdrop-filter': 'blur(20px)',
                  'border-color': 'rgba(255,255,255,0.1)',
                }}
              >
                <div class="flex items-center gap-2 text-[11.5px] text-white/60 mb-1">
                  <span class="w-4 h-4 rounded grid place-items-center text-[9px] font-bold text-white" style={{ background: 'linear-gradient(135deg, oklch(60% 0.18 162), oklch(54% 0.18 195))' }}>M</span>
                  <span>Mail · 5m ago</span>
                </div>
                <div class="text-[14px] font-semibold">(其他通知预览)</div>
              </div>
            </div>
          </div>

          {/* W2-2:待发定时广播队列——可查看 + 取消（闭合 D2 投递调度，避免误排无法撤销） */}
          <Show when={scheduledList().length > 0 || scheduledLoading()}>
            <div class="mt-5 pt-4 border-t border-border-hairline">
              <div class="flex items-center justify-between mb-2">
                <h3 class="text-sm font-semibold">
                  待发排程 <span class="text-xs text-content-tertiary font-normal">（{scheduledList().length}）</span>
                </h3>
                <button type="button" class="text-xs text-accent hover:underline cursor-pointer" onClick={() => void loadScheduled()}>刷新</button>
              </div>
              <Show
                when={scheduledList().length > 0}
                fallback={<div class="text-xs text-content-tertiary py-2">加载中…</div>}
              >
                <div class="space-y-2">
                  <For each={scheduledList()}>
                    {(s) => (
                      <div class="flex items-center justify-between gap-3 rounded-md border border-border-hairline bg-surface px-3 py-2">
                        <div class="min-w-0">
                          <div class="text-sm font-medium truncate">{s.title}</div>
                          <div class="text-[11px] text-content-tertiary mt-0.5">
                            计划 {new Date(s.scheduledAt).toLocaleString()} · 受众 {audienceSummary(s)}
                          </div>
                        </div>
                        <Button size="sm" variant="ghost" onClick={() => setCancelTarget(s)}>取消</Button>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          </Show>
        </Card>
      </div>

      {/* Tabs / Telemetry Panel 不受 loading 影响：刷新时 Telemetry 已展开面板不被卸载 */}
      <Tabs
        tabs={[
          { id: 'sse', label: `SSE 实时连接 (${sseLive().length})` },
          { id: 'recent', label: `近期活跃 (${recentlyActive().length})` },
          { id: 'all', label: '全部设备' },
        ]}
        active={tab()}
        onChange={(id) => setTab(id as 'sse' | 'recent' | 'all')}
      />

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

        {/* m027 / m027-G:全部设备 — 后端分页 + 搜索 + 平台 filter + CSV 导出 + country 列 */}
        <Show when={tab() === 'all'}>
          <Card padding="md">
            <div class="flex gap-2 mb-3 items-center">
              <Input
                size="sm"
                placeholder="搜索设备 ID / 用户 ID"
                value={allQ()}
                onInput={(e) => {
                  batch(() => {
                    setAllQ(e.currentTarget.value);
                    setAllPage(1);
                  });
                }}
                class="max-w-[260px]"
              />
              <select
                class="select select-sm"
                value={allPlatform()}
                onChange={(e) => {
                  batch(() => {
                    setAllPlatform(e.currentTarget.value as any);
                    setAllPage(1);
                  });
                }}
              >
                <option value="">全部平台</option>
                <option value="web">Web</option>
                <option value="ios">iOS</option>
                <option value="android">Android</option>
              </select>
              <div class="ml-auto flex gap-2">
                <Button size="sm" variant="outline" onClick={exportCsv} disabled={allRows().length === 0}>导出 CSV</Button>
              </div>
            </div>
            <Show when={!allLoading()} fallback={<div class="flex justify-center py-8"><Spinner /></div>}>
              <Show
                when={allRows().length > 0}
                fallback={<Empty title="暂无设备记录" description="筛选条件下没有命中数据" />}
              >
                <Table
                  columns={[
                    {
                      key: 'deviceId',
                      title: '设备 ID',
                      render: (r: ListedDevice) => (
                        <span class="font-mono text-xs tabular-nums" title={r.deviceId}>{truncateId(r.deviceId)}</span>
                      ),
                    },
                    {
                      key: 'platform',
                      title: '平台',
                      render: (r: ListedDevice) => <Badge size="sm" variant="default">{r.platform}</Badge>,
                    },
                    {
                      key: 'appVersion',
                      title: '版本',
                      render: (r: ListedDevice) => (
                        <span class="font-mono text-xs tabular-nums">{r.appVersion ?? '—'}</span>
                      ),
                    },
                    {
                      key: 'model',
                      title: '型号',
                      render: (r: ListedDevice) => (
                        <span class="font-mono text-xs tabular-nums">{r.model ?? '未上报'}</span>
                      ),
                    },
                    {
                      key: 'country',
                      title: '国家',
                      render: (r: ListedDevice) => (
                        <span class="text-xs tabular-nums">{r.country ?? '—'}</span>
                      ),
                    },
                    {
                      key: 'userId',
                      title: '用户',
                      render: (r: ListedDevice) => (
                        <span class="font-mono text-xs tabular-nums">{r.userId ? truncateId(r.userId) : '—'}</span>
                      ),
                    },
                    {
                      key: 'lastSeenAt',
                      title: '最后活跃',
                      render: (r: ListedDevice) => (
                        <span class="text-xs tabular-nums whitespace-nowrap">{formatTimestamp(r.lastSeenAt)}</span>
                      ),
                    },
                    {
                      key: 'status',
                      title: '状态',
                      render: (r: ListedDevice) => (
                        <Badge size="sm" variant={r.isBanned ? 'error' : 'success'} dot>
                          {r.isBanned ? '已封禁' : '正常'}
                        </Badge>
                      ),
                    },
                    {
                      key: 'actions',
                      title: '操作',
                      render: (r: ListedDevice) => (
                        <Button size="xs" variant="ghost" onClick={() => setDetailDevice({
                          deviceId: r.deviceId, platform: r.platform, userId: r.userId, appVersion: r.appVersion,
                          isBanned: r.isBanned, lastSeenAt: r.lastSeenAt,
                          // 分页列表端点不透出数据通道,详情抽屉以遥测补全
                          dataChannels: { amas: 'none', learning: 'none', telemetry: 'none' },
                        })}>详情</Button>
                      ),
                    },
                  ]}
                  data={allRows()}
                  aria-label="全部设备分页"
                />
                <div class="flex items-center justify-between mt-3 text-xs">
                  <span class="text-content-tertiary tabular-nums">
                    显示 {(allPage() - 1) * allPerPage() + 1}–{Math.min(allPage() * allPerPage(), allTotal())} 共 {allTotal()} 台
                  </span>
                  <div class="flex gap-2 items-center">
                    <Button size="xs" variant="ghost" disabled={allPage() <= 1} onClick={() => setAllPage(allPage() - 1)}>‹</Button>
                    <span class="tabular-nums">{allPage()} / {Math.max(1, Math.ceil(allTotal() / allPerPage()))}</span>
                    <Button size="xs" variant="ghost"
                      disabled={allPage() >= Math.ceil(allTotal() / allPerPage())}
                      onClick={() => setAllPage(allPage() + 1)}
                    >›</Button>
                  </div>
                </div>
              </Show>
            </Show>
          </Card>
        </Show>
      </Show>

      {/* Telemetry Panel — 不被刷新 loading 卸载，避免已展开的遥测面板因主列表刷新而消失。
          重构:设备画像提顶只显一次 + 系统按 event_type 分类(全量计数) + 每类聚合 + 紧凑行点开展开 */}
      <Show when={telemetryDevice()}>
        <Card class="mt-4">
          <div class="flex items-center justify-between mb-3">
            <h3 class="text-sm font-semibold">
              遥测记录 — <span class="font-mono">{truncateId(telemetryDevice()!)}</span>
              <Show when={telemetrySummary()}>
                <span class="text-content-tertiary font-normal"> (共 {telemetrySummary()!.total} 条)</span>
              </Show>
            </h3>
            <Button size="xs" variant="ghost" onClick={() => setTelemetryDevice(null)}>关闭</Button>
          </div>

          {/* 首屏加载(总览未到):整体 spinner,避免面板体空白 */}
          <Show when={telemetryLoading() && !telemetrySummary()}>
            <div class="flex justify-center py-6"><Spinner /></div>
          </Show>

          {/* 设备画像 + 时间范围:同设备恒定,提到顶部只显示一次(不再每条重复) */}
          <Show when={telemetrySummary()?.deviceProfile}>
            {(dp) => (
              <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-content-secondary bg-surface-secondary rounded px-2.5 py-1.5 mb-3">
                <span class="text-content-tertiary">设备画像</span>
                <Show when={dp().osName}><span class="font-medium text-content-primary">{dp().osName}</span></Show>
                <Show when={dp().browserName}><span>{dp().browserName} {dp().browserVersion}</span></Show>
                <Show when={dp().cpuCores !== null}><span>{dp().cpuCores} 核{dp().memoryGb ? ` / ${dp().memoryGb}GB` : ''}</span></Show>
                <Show when={dp().screenWidth !== null}><span>{dp().screenWidth}×{dp().screenHeight} @{dp().pixelRatio}x</span></Show>
                <Show when={dp().timezone}><span>{dp().timezone} / {dp().language}</span></Show>
                <Show when={telemetrySummary()?.firstTs}>
                  <span class="text-content-tertiary tabular-nums">· {formatTimestamp(telemetrySummary()!.firstTs)} – {formatTimestamp(telemetrySummary()!.lastTs)}</span>
                </Show>
              </div>
            )}
          </Show>

          {/* 操作概览:这台设备做了什么(全量聚合,系统已分好类,无需翻记录) */}
          <Show when={telemetrySummary()}>
            {(s) => (
              <Show when={s().total > 0} fallback={<Empty title="暂无遥测数据" description="该设备尚未上传遥测" />}>
                <div class="rounded-lg border border-border bg-surface-secondary/40 p-3 mb-3 space-y-1.5">
                  <div class="text-xs font-semibold text-content-secondary mb-1">这台设备做了什么</div>
                  {/* 🛠 功能使用(featureUsage 累加) */}
                  <Show when={s().featureUsage.length > 0}>
                    <div class="flex gap-2 text-xs">
                      <span class="text-content-tertiary shrink-0 w-16">🛠 功能</span>
                      <div class="flex flex-wrap gap-1.5">
                        <For each={s().featureUsage}>
                          {(f) => <Badge variant="info" size="sm">{f.name} ×{f.count}</Badge>}
                        </For>
                      </div>
                    </div>
                  </Show>
                  {/* 🧭 访问页面(currentRoute 分组) */}
                  <Show when={s().routes.length > 0}>
                    <div class="flex gap-2 text-xs">
                      <span class="text-content-tertiary shrink-0 w-16">🧭 页面</span>
                      <div class="flex flex-wrap gap-x-3 gap-y-1 text-content-secondary">
                        <For each={s().routes}>
                          {(r) => <span><span class="font-mono">{r.name}</span> <span class="text-content-tertiary">×{r.count}</span></span>}
                        </For>
                      </div>
                    </div>
                  </Show>
                  {/* 🖱 点击(总数 + clickTargets 热点) */}
                  <Show when={s().totalClicks > 0 || s().clickTargets.length > 0}>
                    <div class="flex gap-2 text-xs">
                      <span class="text-content-tertiary shrink-0 w-16">🖱 点击</span>
                      <div class="flex flex-wrap gap-x-3 gap-y-1 text-content-secondary">
                        <span>共 {s().totalClicks} 次</span>
                        <For each={s().clickTargets}>
                          {(c) => <span class="text-content-tertiary">{c.name} ×{c.count}</span>}
                        </For>
                      </div>
                    </div>
                  </Show>
                  {/* ⚠ 异常(错误总数 + 事件类型分布) */}
                  <div class="flex gap-2 text-xs">
                    <span class="text-content-tertiary shrink-0 w-16">⚠ 异常</span>
                    <div class="flex flex-wrap gap-x-3 gap-y-1 text-content-secondary">
                      <span><span class="text-content-tertiary">错误 </span><span class={s().totalErrors > 0 ? 'text-error font-medium' : ''}>{s().totalErrors}</span></span>
                      <For each={s().byEventType}>
                        {(e) => <span class="text-content-tertiary"><span class="font-mono">{e.eventType}</span>×{e.count}</span>}
                      </For>
                    </div>
                  </div>
                  {/* ⏱ 活跃(总时长 + 会话数) */}
                  <div class="flex gap-2 text-xs">
                    <span class="text-content-tertiary shrink-0 w-16">⏱ 活跃</span>
                    <div class="flex flex-wrap gap-x-3 gap-y-1 text-content-secondary">
                      <span><span class="text-content-tertiary">总时长 </span>{fmtDuration(s().totalDurationSecs)}</span>
                      <span><span class="text-content-tertiary">会话 </span>{s().sessionCount}</span>
                    </div>
                  </div>
                </div>
              </Show>
            )}
          </Show>

          {/* 原始明细折叠:分类 chips + 记录列表默认收起,要逐条翻才展开 */}
          <Show when={telemetrySummary() && telemetrySummary()!.total > 0}>
            <button
              type="button"
              class="text-xs text-content-secondary hover:text-content-primary inline-flex items-center gap-1.5 mb-2"
              aria-expanded={showDetail()}
              onClick={() => setShowDetail((v) => !v)}
            >
              <svg class={`w-3.5 h-3.5 transition-transform ${showDetail() ? 'rotate-90' : ''}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
              </svg>
              原始明细（{telemetrySummary()!.total} 条）
            </button>
          </Show>

          <Show when={showDetail()}>
          {/* 系统分类 chips:按 event_type 全量计数,带类别色点 */}
          <Show when={telemetrySummary() && telemetrySummary()!.byEventType.length > 0}>
            <div class="flex flex-wrap gap-1.5 mb-2">
              <button
                type="button"
                class={`px-2.5 py-0.5 rounded-full text-xs border transition-colors ${telemetryFilter() === '' ? 'bg-accent text-white border-accent' : 'border-border text-content-secondary hover:bg-surface-secondary'}`}
                onClick={() => selectTelemetryFilter('')}
              >
                全部 {telemetrySummary()!.total}
              </button>
              <For each={telemetrySummary()!.byEventType}>
                {(st) => {
                  const cat = evtCategory(st.eventType);
                  return (
                    <button
                      type="button"
                      title={`系统分类:${CAT_LABEL[cat]}`}
                      class={`px-2.5 py-0.5 rounded-full text-xs border inline-flex items-center gap-1.5 transition-colors ${telemetryFilter() === st.eventType ? 'bg-accent text-white border-accent' : 'border-border text-content-secondary hover:bg-surface-secondary'}`}
                      onClick={() => selectTelemetryFilter(st.eventType)}
                    >
                      <span class={`w-1.5 h-1.5 rounded-full ${telemetryFilter() === st.eventType ? 'bg-white' : CAT_DOT[cat]}`} />
                      <span class="font-mono">{st.eventType}</span> {st.count}
                    </button>
                  );
                }}
              </For>
            </div>
          </Show>

          {/* 每类聚合行:选中具体类型显该类指标;全部时显类型数 + 总错误 */}
          <Show when={telemetrySummary()}>
            <div class="text-xs text-content-secondary mb-3 flex flex-wrap gap-x-4 gap-y-1">
              <Show
                when={selectedStat()}
                fallback={
                  <>
                    <span><span class="text-content-tertiary">类型 </span>{telemetrySummary()!.byEventType.length} 种</span>
                    <span><span class="text-content-tertiary">总错误 </span><span class={telemetrySummary()!.byEventType.reduce((a, s) => a + s.totalErrors, 0) > 0 ? 'text-error font-medium' : ''}>{telemetrySummary()!.byEventType.reduce((a, s) => a + s.totalErrors, 0)}</span></span>
                  </>
                }
              >
                {(st) => (
                  <>
                    <span><span class="text-content-tertiary">平均时长 </span>{st().avgDurationSecs.toFixed(0)}s</span>
                    <span><span class="text-content-tertiary">总错误 </span><span class={st().totalErrors > 0 ? 'text-error font-medium' : ''}>{st().totalErrors}</span></span>
                    <span><span class="text-content-tertiary">操作/分 </span>{st().avgActionsPerMin.toFixed(1)}</span>
                    <span><span class="text-content-tertiary">响应 </span>{st().avgResponseMs.toFixed(0)}ms</span>
                  </>
                )}
              </Show>
            </div>
          </Show>

          {/* 记录列表:紧凑行 + 点开展开。loading 只罩列表,分类 chips/画像不闪 */}
          <Show when={!telemetryLoading()} fallback={<div class="flex justify-center py-4"><Spinner /></div>}>
            <Show when={telemetryRecords().length > 0} fallback={<Empty title="暂无遥测记录" description="该设备尚未上传遥测数据或无匹配分类" />}>
              <div class="space-y-1 max-h-96 overflow-y-auto">
                <For each={telemetryRecords()}>
                  {(record) => {
                    const cat = evtCategory(record.eventType);
                    const expanded = () => expandedTel().has(record.id);
                    return (
                      <Card variant="outlined" padding="sm" class="text-xs">
                        {/* 紧凑行:点击整行展开 */}
                        <button
                          type="button"
                          class="w-full flex items-center gap-2 min-w-0 text-left"
                          aria-expanded={expanded()}
                          onClick={() => toggleTelExpand(record.id)}
                        >
                          <span class={`w-1.5 h-1.5 rounded-full shrink-0 ${CAT_DOT[cat]}`} />
                          <Badge variant={CAT_VARIANT[cat]} size="sm"><span class="font-mono">{record.eventType}</span></Badge>
                          <span class="flex items-center gap-x-3 flex-wrap text-content-secondary min-w-0">
                            <span><span class="text-content-tertiary">时长 </span>{record.sessionStats.sessionDurationSecs}s</span>
                            <span><span class="text-content-tertiary">错误 </span><span class={record.sessionStats.errorCount > 0 ? 'text-error font-medium' : ''}>{record.sessionStats.errorCount}</span></span>
                            <Show when={record.sessionStats.actionsPerMin > 0}>
                              <span><span class="text-content-tertiary">操作/分 </span>{record.sessionStats.actionsPerMin.toFixed(1)}</span>
                            </Show>
                            <Show when={record.behaviorSummary.currentRoute}>
                              <span class="font-mono text-content-tertiary truncate max-w-[10rem]">{record.behaviorSummary.currentRoute}</span>
                            </Show>
                          </span>
                          <span class="ml-auto flex items-center gap-1.5 shrink-0">
                            <span class="tabular-nums whitespace-nowrap text-content-tertiary">{formatTimestamp(record.serverTs)}</span>
                            <svg class={`w-3.5 h-3.5 transition-transform ${expanded() ? 'rotate-90' : ''}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                              <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
                            </svg>
                          </span>
                        </button>

                        {/* 展开:完整会话统计 + 行为摘要 + 功能使用(设备信息已提顶,不再重复) */}
                        <Show when={expanded()}>
                          <div class="mt-2 space-y-2">
                            <div class="bg-surface-secondary rounded p-1.5 space-y-0.5">
                              <div class="font-semibold text-accent mb-1">会话统计</div>
                              <div class="flex gap-4 flex-wrap">
                                <span><span class="text-content-tertiary">时长 </span>{record.sessionStats.sessionDurationSecs}s</span>
                                <span><span class="text-content-tertiary">操作/分 </span>{record.sessionStats.actionsPerMin.toFixed(1)}</span>
                                <span><span class="text-content-tertiary">错误 </span>{record.sessionStats.errorCount}</span>
                                <span><span class="text-content-tertiary">响应 </span>{record.sessionStats.avgResponseTimeMs.toFixed(0)}ms</span>
                              </div>
                            </div>
                            <Show when={record.behaviorSummary.currentRoute !== null}>
                              <div class="bg-surface-secondary rounded p-1.5 space-y-0.5">
                                <div class="font-semibold text-success mb-1">行为摘要</div>
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
                            <Show when={Object.keys(record.featureUsage).length > 0}>
                              <div class="bg-surface-secondary rounded p-1.5">
                                <div class="font-semibold text-warning mb-1">功能使用</div>
                                <div class="flex gap-3 flex-wrap">
                                  <For each={Object.entries(record.featureUsage)}>
                                    {([k, v]) => <span class="tabular-nums"><span class="text-content-tertiary">{k} </span>{v}</span>}
                                  </For>
                                </div>
                              </div>
                            </Show>
                          </div>
                        </Show>
                      </Card>
                    );
                  }}
                </For>
                {/* 50 条分页上限的诚实提示:当前(过滤后)总数 > 已加载条数时标注 */}
                <Show when={telemetryTotal() > telemetryRecords().length}>
                  <div class="text-center text-content-tertiary py-1">
                    显示最近 {telemetryRecords().length} 条 · 共 {telemetryTotal()} 条
                  </div>
                </Show>
              </div>
            </Show>
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

      {/* m027:强制升级广播 Confirm Dialog */}
      <Show when={broadcastUpgradeOpen()}>
        {(target) => (
          <ConfirmDialog
            open={true}
            title="确认推送强制升级"
            message={
              <span>
                平台 <strong>{target().platform.toUpperCase()}</strong>:
                所有低于 <span class="font-mono">{target().below}</span> 的设备将收到 SSE 升级提示,目标版本 <span class="font-mono">{target().latest}</span>。
              </span>
            }
            confirmText={broadcastUpgradeSending() ? '推送中…' : '确认推送'}
            variant="warning"
            onConfirm={sendBroadcastUpgrade}
            onCancel={() => { setBroadcastUpgradeOpen(null); setBroadcastUpgradeMessage(''); }}
          >
            <Input
              type="text"
              placeholder="附加消息(可选, 显示在客户端横幅)"
              value={broadcastUpgradeMessage()}
              onInput={(e) => setBroadcastUpgradeMessage(e.currentTarget.value)}
              maxlength={300}
            />
          </ConfirmDialog>
        )}
      </Show>

      {/* W2-2:取消定时广播 Confirm Dialog */}
      <Show when={cancelTarget()}>
        {(t) => (
          <ConfirmDialog
            open={true}
            title="取消该定时广播？"
            message={
              <span>
                「<strong>{t().title}</strong>」计划于 <span class="font-mono">{new Date(t().scheduledAt).toLocaleString()}</span> 下发，
                取消后将不再发出。
              </span>
            }
            confirmText={canceling() ? '取消中…' : '确认取消'}
            cancelText="返回"
            variant="danger"
            loading={canceling()}
            onConfirm={confirmCancelScheduled}
            onCancel={() => setCancelTarget(null)}
          />
        )}
      </Show>
    </div>
  );
}
