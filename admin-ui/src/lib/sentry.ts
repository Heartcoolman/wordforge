// Sentry SDK 初始化封装（v1.1-P2.6）
//
// 设计：
// - DSN 缺省（本地开发 / 测试 / 没有 Sentry 项目）时 init 跳过，所有 capture 调用退化为 no-op
// - 现有 /api/telemetry/error best-effort 上报保留作 fallback（见 components/ErrorBoundary.tsx）
// - 这里仅做客户端浏览器 SDK 初始化，不引入 tracing / replay 等附加包，控制 bundle 体积

import * as Sentry from '@sentry/browser';

let initialized = false;

export function initSentry(): void {
  if (initialized) return;
  const dsn = import.meta.env.VITE_SENTRY_DSN;
  if (!dsn) {
    // DSN 缺省：本地开发不依赖外部服务，保持 no-op
    return;
  }

  const tracesSampleRateRaw = import.meta.env.VITE_SENTRY_TRACES_SAMPLE_RATE;
  const tracesSampleRate = tracesSampleRateRaw ? Number(tracesSampleRateRaw) : 0;

  Sentry.init({
    dsn,
    environment: import.meta.env.VITE_SENTRY_ENV ?? (import.meta.env.DEV ? 'development' : 'production'),
    release: import.meta.env.VITE_SENTRY_RELEASE,
    // 默认不开启 performance tracing（NaN / 0 等价 disabled）
    tracesSampleRate: Number.isFinite(tracesSampleRate) ? tracesSampleRate : 0,
    // 限制 breadcrumb 数量，避免堆积过多 UI 事件影响内存
    maxBreadcrumbs: 50,
  });

  initialized = true;
}

export function isSentryEnabled(): boolean {
  return initialized;
}

// 透传 captureException，供 ErrorBoundary 等模块统一调用
export function captureException(err: unknown, context?: Record<string, unknown>): void {
  if (!initialized) return;
  Sentry.captureException(err, context ? { contexts: { solid: context } } : undefined);
}
