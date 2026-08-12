/**
 * 运维告警桥：消费 admin SSE 的 incident / worker_missed / llm_budget_exceeded
 * 三个运维事件，复用全局 toast（uiStore.toast + <Toaster/>）做轻量告警条。
 * 与探针桥同样在 App 挂载时全局启动一次。
 */

import { connectSseStream } from '@/api/http';
import { uiStore } from '@/stores/ui';

let stopSseStream: (() => void) | undefined;

export function startOpsAlerts(): void {
  if (stopSseStream) return;
  stopSseStream = connectSseStream({
    onIncident: ({ errorRate, windowSecs }) => {
      uiStore.toast.error(
        '5xx 错误率告警',
        `近 ${windowSecs}s 窗口错误率 ${(errorRate * 100).toFixed(2)}%，请检查监控面板`,
      );
    },
    onWorkerMissed: ({ workerName, missCount }) => {
      uiStore.toast.warning(
        '后台任务调度异常',
        `worker「${workerName}」已连续 ${missCount} 个周期未上报`,
      );
    },
    onLlmBudgetExceeded: ({ spentYuan, capYuan, resumeMonth }) => {
      uiStore.toast.warning(
        'LLM 月度预算超限',
        `本月已花费 ¥${spentYuan.toFixed(2)}（上限 ¥${capYuan.toFixed(2)}），advisor 已停跑，${resumeMonth} 恢复`,
      );
    },
  });
}

export function stopOpsAlerts(): void {
  stopSseStream?.();
  stopSseStream = undefined;
}
