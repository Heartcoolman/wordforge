import { createSignal, onMount, Show, For } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { adminApi, type SseLiveEntry, type RecentlyActiveEntry, type TelemetryRecord, type DataChannelValue } from '@/api/admin';
import { uiStore } from '@/stores/ui';

const CHANNEL_LABELS: Record<string, string> = {
  amas: 'AMAS',
  learning: '学习',
  telemetry: '遥测',
};

const STATUS_COLORS: Record<DataChannelValue, string> = {
  uploaded: 'bg-success-light text-success',
  nil: 'bg-warning-light text-warning',
  none: 'bg-error-light text-error',
};

const STATUS_TEXT: Record<DataChannelValue, string> = {
  uploaded: '已上传',
  nil: '空数据',
  none: '未上传',
};

function DataChannelBadge(props: { channel: string; status: DataChannelValue }) {
  return (
    <span
      class={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium ${STATUS_COLORS[props.status]}`}
      title={`${CHANNEL_LABELS[props.channel]}: ${STATUS_TEXT[props.status]}`}
    >
      <span class="w-1.5 h-1.5 rounded-full" classList={{
        'bg-success': props.status === 'uploaded',
        'bg-warning': props.status === 'nil',
        'bg-error': props.status === 'none',
      }} />
      {CHANNEL_LABELS[props.channel]}
    </span>
  );
}

export default function ClientsPage() {
  const [sseLive, setSseLive] = createSignal<SseLiveEntry[]>([]);
  const [recentlyActive, setRecentlyActive] = createSignal<RecentlyActiveEntry[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [tab, setTab] = createSignal<'sse' | 'recent'>('sse');

  // Ban confirm
  const [banTarget, setBanTarget] = createSignal<{ id: string; action: 'ban' | 'unban' } | null>(null);
  const [banReason, setBanReason] = createSignal('');

  // Telemetry
  const [telemetryDevice, setTelemetryDevice] = createSignal<string | null>(null);
  const [telemetryRecords, setTelemetryRecords] = createSignal<TelemetryRecord[]>([]);
  const [telemetryTotal, setTelemetryTotal] = createSignal(0);
  const [telemetryLoading, setTelemetryLoading] = createSignal(false);

  const loadClients = async () => {
    try {
      setLoading(true);
      const data = await adminApi.getClients();
      setSseLive(data.sseLive);
      setRecentlyActive(data.recentlyActive);
    } catch (e: any) {
      uiStore.toast.error('加载失败', e.message);
    } finally {
      setLoading(false);
    }
  };

  const handleBan = async () => {
    const t = banTarget();
    if (!t) return;
    try {
      if (t.action === 'ban') {
        await adminApi.banClient(t.id, banReason() || undefined);
        uiStore.toast.success('已封禁设备');
      } else {
        await adminApi.unbanClient(t.id);
        uiStore.toast.success('已解封设备');
      }
      setBanTarget(null);
      setBanReason('');
      loadClients();
    } catch (e: any) {
      uiStore.toast.error('操作失败', e.message);
    }
  };

  const requestTelemetry = async (deviceId: string) => {
    try {
      await adminApi.requestTelemetry(deviceId);
      uiStore.toast.success('已发送遥测请求');
    } catch (e: any) {
      uiStore.toast.error('遥测请求失败', e.message);
    }
  };

  const loadTelemetry = async (deviceId: string) => {
    try {
      setTelemetryLoading(true);
      setTelemetryDevice(deviceId);
      setTelemetryRecords([]);
      setTelemetryTotal(0);
      const data = await adminApi.getTelemetry(deviceId);
      setTelemetryRecords(data.records);
      setTelemetryTotal(data.total);
    } catch (e: any) {
      uiStore.toast.error('加载遥测数据失败', e.message);
    } finally {
      setTelemetryLoading(false);
    }
  };

  onMount(loadClients);

  const truncateId = (id: string | null | undefined) => {
    if (!id) return '';
    return id.length > 12 ? `${id.slice(0, 8)}...${id.slice(-4)}` : id;
  };

  return (
    <div class="space-y-6">
      <div class="flex items-center justify-between">
        <h2 class="text-xl font-bold text-content">客户端管理</h2>
        <Button size="sm" variant="outline" onClick={loadClients}>刷新</Button>
      </div>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        {/* Tabs */}
        <div class="flex gap-2 border-b border-border">
          <button
            class={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${tab() === 'sse' ? 'border-accent text-accent' : 'border-transparent text-content-secondary hover:text-content'}`}
            onClick={() => setTab('sse')}
          >
            SSE 实时连接 ({sseLive().length})
          </button>
          <button
            class={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${tab() === 'recent' ? 'border-accent text-accent' : 'border-transparent text-content-secondary hover:text-content'}`}
            onClick={() => setTab('recent')}
          >
            近期活跃 ({recentlyActive().length})
          </button>
        </div>

        {/* SSE Live */}
        <Show when={tab() === 'sse'}>
          <Show when={sseLive().length > 0} fallback={<p class="text-content-secondary text-sm py-4">暂无活跃 SSE 连接</p>}>
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
                <thead>
<tr class="border-b border-border text-left text-content-secondary">
                     <th class="py-2 pr-4">设备 ID</th>
                     <th class="py-2 pr-4">平台</th>
                     <th class="py-2 pr-4">用户</th>
                     <th class="py-2 pr-4">连接时长</th>
                     <th class="py-2 pr-4">连接数</th>
                     <th class="py-2 pr-4">数据状态</th>
                     <th class="py-2">操作</th>
                   </tr>
                </thead>
                <tbody>
                  <For each={sseLive()}>
                    {(entry) => (
                      <tr class="border-b border-border/50">
                        <td class="py-2 pr-4 font-mono text-xs" title={entry.deviceId}>{truncateId(entry.deviceId)}</td>
                        <td class="py-2 pr-4">{entry.platform}</td>
                        <td class="py-2 pr-4 font-mono text-xs">{truncateId(entry.userId)}</td>
                        <td class="py-2 pr-4">{Math.floor(entry.connectedSecs / 60)}m</td>
<td class="py-2 pr-4">{entry.connectionCount}</td>
                         <td class="py-2 pr-4">
                           <div class="flex gap-1">
                             <DataChannelBadge channel="amas" status={entry.dataChannels.amas} />
                             <DataChannelBadge channel="learning" status={entry.dataChannels.learning} />
                             <DataChannelBadge channel="telemetry" status={entry.dataChannels.telemetry} />
                           </div>
                         </td>
                         <td class="py-2 flex gap-1">
                          <Show when={entry.isBanned} fallback={
                            <Button size="xs" variant="danger" onClick={() => setBanTarget({ id: entry.deviceId, action: 'ban' })}>封禁</Button>
                          }>
                            <Button size="xs" variant="success" onClick={() => setBanTarget({ id: entry.deviceId, action: 'unban' })}>解封</Button>
                          </Show>
                          <Button size="xs" variant="outline" onClick={() => requestTelemetry(entry.deviceId)}>拉取遥测</Button>
                          <Button size="xs" variant="ghost" onClick={() => loadTelemetry(entry.deviceId)}>历史</Button>
                        </td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </Show>
        </Show>

        {/* Recently Active */}
        <Show when={tab() === 'recent'}>
          <Show when={recentlyActive().length > 0} fallback={<p class="text-content-secondary text-sm py-4">暂无近期活跃设备</p>}>
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
                <thead>
<tr class="border-b border-border text-left text-content-secondary">
                     <th class="py-2 pr-4">设备 ID</th>
                     <th class="py-2 pr-4">平台</th>
                     <th class="py-2 pr-4">用户</th>
                     <th class="py-2 pr-4">最后活跃</th>
                     <th class="py-2 pr-4">状态</th>
                     <th class="py-2 pr-4">数据状态</th>
                     <th class="py-2">操作</th>
                   </tr>
                </thead>
                <tbody>
                  <For each={recentlyActive()}>
                    {(entry) => (
                      <tr class="border-b border-border/50">
                        <td class="py-2 pr-4 font-mono text-xs" title={entry.deviceId}>{truncateId(entry.deviceId)}</td>
                        <td class="py-2 pr-4">{entry.platform}</td>
                        <td class="py-2 pr-4 font-mono text-xs">{entry.userId ? truncateId(entry.userId) : '-'}</td>
                        <td class="py-2 pr-4 text-xs">{new Date(entry.lastSeenAt.replace(' ', 'T') + 'Z').toLocaleString()}</td>
<td class="py-2 pr-4">
                           <span class={`inline-block px-2 py-0.5 rounded text-xs font-medium ${entry.isBanned ? 'bg-error-light text-error' : 'bg-success-light text-success'}`}>
                             {entry.isBanned ? '已封禁' : '正常'}
                           </span>
                         </td>
                         <td class="py-2 pr-4">
                           <div class="flex gap-1">
                             <DataChannelBadge channel="amas" status={entry.dataChannels.amas} />
                             <DataChannelBadge channel="learning" status={entry.dataChannels.learning} />
                             <DataChannelBadge channel="telemetry" status={entry.dataChannels.telemetry} />
                           </div>
                         </td>
                         <td class="py-2 flex gap-1">
                          <Show when={entry.isBanned} fallback={
                            <Button size="xs" variant="danger" onClick={() => setBanTarget({ id: entry.deviceId, action: 'ban' })}>封禁</Button>
                          }>
                            <Button size="xs" variant="success" onClick={() => setBanTarget({ id: entry.deviceId, action: 'unban' })}>解封</Button>
                          </Show>
                          <Button size="xs" variant="ghost" onClick={() => loadTelemetry(entry.deviceId)}>历史</Button>
                        </td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </Show>
        </Show>

        {/* Telemetry Panel */}
        <Show when={telemetryDevice()}>
          <Card class="mt-4">
            <div class="flex items-center justify-between mb-3">
              <h3 class="text-sm font-semibold">遥测记录 — <span class="font-mono">{truncateId(telemetryDevice()!)}</span> (共 {telemetryTotal()} 条)</h3>
              <Button size="xs" variant="ghost" onClick={() => setTelemetryDevice(null)}>关闭</Button>
            </div>
            <Show when={!telemetryLoading()} fallback={<div class="flex justify-center py-4"><Spinner /></div>}>
              <Show when={telemetryRecords().length > 0} fallback={<p class="text-content-secondary text-sm">暂无遥测记录</p>}>
                <div class="space-y-2 max-h-80 overflow-y-auto">
                  <For each={telemetryRecords()}>
                    {(record) => (
                      <div class="text-xs border border-border/50 rounded p-2">
                        <div class="flex justify-between text-content-secondary mb-1">
                          <span>{record.eventType}{record.triggeredByRequestId ? ' (按需)' : ''}</span>
                          <span>{new Date(record.serverTs.replace(' ', 'T') + 'Z').toLocaleString()}</span>
                        </div>
                        <pre class="text-content text-xs overflow-x-auto">{JSON.stringify(record.payload, null, 2)}</pre>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </Card>
        </Show>
      </Show>

      {/* Ban Confirm Dialog */}
      <Show when={banTarget()}>
        {(target) => (
          <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => { setBanTarget(null); setBanReason(''); }}>
            <Card variant="elevated" class="max-w-sm mx-4" onClick={(e: MouseEvent) => e.stopPropagation()}>
              <h3 class="text-lg font-semibold text-content mb-2">
                {target().action === 'ban' ? '确认封禁设备' : '确认解封设备'}
              </h3>
              <p class="text-sm text-content-secondary mb-3">
                设备 ID: <span class="font-mono">{truncateId(target().id)}</span>
              </p>
              <Show when={target().action === 'ban'}>
                <input
                  type="text"
                  placeholder="封禁原因（可选）"
                  value={banReason()}
                  onInput={(e) => setBanReason(e.currentTarget.value)}
                  class="w-full px-3 py-2 text-sm border border-border rounded-lg bg-surface text-content mb-3"
                  maxlength={500}
                />
              </Show>
              <div class="flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => { setBanTarget(null); setBanReason(''); }}>取消</Button>
                <Button size="sm" variant={target().action === 'ban' ? 'danger' : 'success'} onClick={handleBan}>
                  {target().action === 'ban' ? '确认封禁' : '确认解封'}
                </Button>
              </div>
            </Card>
          </div>
        )}
      </Show>
    </div>
  );
}
