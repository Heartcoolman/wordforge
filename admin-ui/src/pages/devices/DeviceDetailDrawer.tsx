import { createEffect, createSignal, Show, For } from 'solid-js';
import { Modal } from '@/components/ui/Modal';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi, type ClientDetail, type TelemetrySummary, type DataChannelStatus, type DataChannelValue } from '@/api/admin';
import { uiStore } from '@/stores/ui';

/** 抽屉打开时从列表行直接带入的设备快照,用于在详情端点返回前即时渲染骨架。 */
export interface DeviceDetailSeed {
  deviceId: string;
  platform: string;
  userId: string | null;
  appVersion: string | null | undefined;
  isBanned: boolean;
  dataChannels: DataChannelStatus;
  connectionCount?: number;
  connectedSecs?: number;
  lastSeenAt?: string;
}

const CHANNEL_LABELS: Record<string, string> = { amas: 'AMAS', learning: '学习', telemetry: '遥测' };
const CHANNEL_VARIANT: Record<DataChannelValue, 'success' | 'warning' | 'error'> = {
  uploaded: 'success',
  nil: 'warning',
  none: 'error',
};
const STATUS_TEXT: Record<DataChannelValue, string> = { uploaded: '已上传', nil: '空数据', none: '未上传' };

const fmt = (s: string | null | undefined): string => {
  if (!s) return '—';
  const iso = s.includes('T') ? s : s.replace(' ', 'T') + 'Z';
  const d = new Date(iso);
  return isNaN(d.getTime()) ? s : d.toLocaleString();
};

function Field(props: { label: string; children: any }) {
  return (
    <div>
      <div class="text-[11px] text-content-tertiary mb-0.5">{props.label}</div>
      <div class="text-[13px] font-medium break-all">{props.children}</div>
    </div>
  );
}

/**
 * m032:单设备详情抽屉。接入 GET /api/admin/clients/:id —— 该端点透出列表/遥测均不提供的
 * 详情独有字段:country(GeoIP)、online、firstSeenAt、banned_at/ban_reason 及遥测摘要。
 * 行快照(seed)仅用于端点返回前的即时骨架渲染,返回后以 detail 为准。
 */
export function DeviceDetailDrawer(props: { device: DeviceDetailSeed | null; onClose: () => void }) {
  const [detail, setDetail] = createSignal<ClientDetail | null>(null);
  const [loading, setLoading] = createSignal(false);
  let reqId = 0;

  createEffect(() => {
    const dev = props.device;
    if (!dev) {
      setDetail(null);
      return;
    }
    const my = ++reqId;
    setLoading(true);
    setDetail(null);
    adminApi
      .getClientDetail(dev.deviceId)
      .then((res) => {
        if (my === reqId) setDetail(res);
      })
      .catch((e: any) => {
        if (my === reqId) uiStore.toast.error('加载设备详情失败', e?.message);
      })
      .finally(() => {
        if (my === reqId) setLoading(false);
      });
  });

  // 详情返回后从 detail 取最新一条遥测明细(端点已内联摘要)。
  const telemetry = (): TelemetrySummary | null => detail()?.telemetry.latest ?? null;

  return (
    <Modal open={!!props.device} onClose={props.onClose} size="xl" title="设备详情">
      <Show when={props.device} fallback={<Empty title="无设备" description="" />}>
        {(d) => (
          <div class="space-y-5 pt-1">
            <div class="flex items-center gap-2 flex-wrap">
              <Badge variant="default" size="md">{detail()?.platform ?? d().platform}</Badge>
              <Badge variant={(detail()?.isBanned ?? d().isBanned) ? 'error' : 'success'} size="md" dot>
                {(detail()?.isBanned ?? d().isBanned) ? '已封禁' : '正常'}
              </Badge>
              <Show when={detail()}>
                {(dt) => (
                  <Badge variant={dt().online ? 'success' : 'default'} size="md" dot>
                    {dt().online ? '在线' : '离线'}
                  </Badge>
                )}
              </Show>
              <span class="font-mono text-xs text-content-tertiary break-all">{d().deviceId}</span>
            </div>

            <div class="grid grid-cols-2 md:grid-cols-3 gap-3">
              <Field label="用户 ID">{detail()?.userId ?? d().userId ?? '—'}</Field>
              <Field label="App 版本"><span class="font-mono">{detail()?.appVersion ?? d().appVersion ?? '—'}</span></Field>
              <Field label="设备型号"><span class="font-mono">{detail()?.model ?? '未上报'}</span></Field>
              <Field label="国家 / 地区">
                <Show when={detail()?.country} fallback={<span title="该设备暂无可反查的 GeoIP 样本">—</span>}>
                  <span class="font-mono">{detail()!.country}</span>
                </Show>
              </Field>
              <Field label="活跃连接数">
                <span class="tabular-nums">{detail()?.connectionCount ?? d().connectionCount ?? 0}</span>
              </Field>
              <Show when={d().connectedSecs != null}>
                <Field label="连接时长"><span class="tabular-nums">{Math.floor((d().connectedSecs ?? 0) / 60)} 分钟</span></Field>
              </Show>
              <Field label="首次见到">{fmt(detail()?.firstSeenAt)}</Field>
              <Field label="最后活跃">{fmt(detail()?.lastSeenAt ?? d().lastSeenAt)}</Field>
              <Show when={detail()?.bannedAt}>
                <Field label="封禁时间">{fmt(detail()!.bannedAt)}</Field>
              </Show>
              <Show when={detail()?.banReason}>
                <Field label="封禁原因"><span class="text-error-strong">{detail()!.banReason}</span></Field>
              </Show>
            </div>

            <div>
              <div class="text-[11px] font-semibold uppercase tracking-wider text-content-secondary mb-1.5">数据通道</div>
              <div class="flex flex-wrap gap-1.5">
                <For each={['amas', 'learning', 'telemetry'] as const}>
                  {(ch) => (
                    <Badge variant={CHANNEL_VARIANT[d().dataChannels[ch]]} size="sm" dot>
                      {CHANNEL_LABELS[ch]} · {STATUS_TEXT[d().dataChannels[ch]]}
                    </Badge>
                  )}
                </For>
              </div>
            </div>

            <div>
              <div class="text-[11px] font-semibold uppercase tracking-wider text-content-secondary mb-1.5">
                最新遥测明细
                <Show when={detail()}>
                  <span class="ml-1.5 font-normal normal-case tracking-normal text-content-tertiary">共 {detail()!.telemetry.total} 条</span>
                </Show>
              </div>
              <Show when={!loading()} fallback={<div class="flex justify-center py-6"><Spinner /></div>}>
                <Show when={telemetry()} fallback={<div class="text-xs text-content-tertiary">该设备暂无遥测明细</div>}>
                  {(t) => (
                    <div class="grid grid-cols-2 md:grid-cols-3 gap-3 border border-border-hairline rounded-lg p-3">
                      <Field label="事件"><Badge variant="default" size="sm">{t().eventType}</Badge></Field>
                      <Field label="采集时间">{fmt(t().serverTs)}</Field>
                      <Show when={t().deviceProfile.osName}>
                        <Field label="系统">{t().deviceProfile.osName}</Field>
                      </Show>
                      <Show when={t().deviceProfile.browserName}>
                        <Field label="浏览器">{t().deviceProfile.browserName} {t().deviceProfile.browserVersion ?? ''}</Field>
                      </Show>
                      <Show when={t().deviceProfile.timezone}>
                        <Field label="时区 / 语言">{t().deviceProfile.timezone} / {t().deviceProfile.language ?? '—'}</Field>
                      </Show>
                      <Show when={t().deviceProfile.screenWidth != null}>
                        <Field label="分辨率"><span class="font-mono tabular-nums">{t().deviceProfile.screenWidth}×{t().deviceProfile.screenHeight}</span></Field>
                      </Show>
                      <Field label="会话时长"><span class="tabular-nums">{t().sessionStats.sessionDurationSecs}s</span></Field>
                      <Field label="错误数"><span class="tabular-nums">{t().sessionStats.errorCount}</span></Field>
                    </div>
                  )}
                </Show>
              </Show>
            </div>
          </div>
        )}
      </Show>
    </Modal>
  );
}
