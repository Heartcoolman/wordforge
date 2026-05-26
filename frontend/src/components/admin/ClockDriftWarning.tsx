import { Show, createSignal, onCleanup, onMount } from 'solid-js';
import { api } from '@/api/client';

/**
 * Admin 后台顶部时钟漂移告警条。
 *
 * 后端 `/health` 暴露 `services.clock` 块（status / driftSecs / thresholdSecs）。
 * 当服务器自检发现本机时钟与公网 NTP 漂移超阈时显示，提示运维处理。
 *
 * 不暴露给普通用户：仅在 Admin 后台挂载。
 */

interface ClockBlock {
  status: 'unknown' | 'healthy' | 'drifted' | 'failed';
  driftSecs: number;
  thresholdSecs: number;
  lastCheckAt: number | null;
}

interface HealthResponse {
  services?: { clock?: ClockBlock };
}

/** 轮询间隔：5 分钟。漂移本身就是慢变信号，没必要更频繁。 */
const POLL_INTERVAL_MS = 5 * 60 * 1000;

export function ClockDriftWarning() {
  const [clock, setClock] = createSignal<ClockBlock | null>(null);

  async function fetchOnce() {
    try {
      // 注意：/health 是公开端点，不需要 admin token
      const data = await api.get<HealthResponse>('/health');
      const c = data?.services?.clock;
      if (c && typeof c.driftSecs === 'number') {
        setClock(c);
      }
    } catch {
      /* 网络/解析错误：不阻塞 UI，下个 tick 再试 */
    }
  }

  let timer: number | undefined;
  onMount(() => {
    fetchOnce();
    timer = window.setInterval(fetchOnce, POLL_INTERVAL_MS);
  });
  onCleanup(() => {
    if (timer !== undefined) window.clearInterval(timer);
  });

  return (
    <Show when={clock()?.status === 'drifted'}>
      <div
        role="alert"
        class="bg-error-light border-b border-error/30 text-error px-4 py-2 text-sm flex items-start gap-2"
      >
        <svg class="w-4 h-4 mt-0.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <div class="min-w-0">
          <strong>服务器时钟漂移 {clock()?.driftSecs}s</strong>
          （阈值 {clock()?.thresholdSecs}s）。这会导致管理员登录失败、JWT 过早过期等问题。
          请在服务器上执行 <code class="font-mono text-xs bg-error/10 px-1 rounded">sudo ntpdate -u pool.ntp.org</code> 同步时钟。
        </div>
      </div>
    </Show>
  );
}
