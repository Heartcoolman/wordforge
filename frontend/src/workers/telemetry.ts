import { api } from '@/api/client';
import { getDeviceId } from '@/lib/device';
import { tokenManager } from '@/lib/token';

const INTERVAL_MS = 5 * 60 * 1000;

let timer: ReturnType<typeof setInterval> | undefined;
let sessionStart = Date.now();
let actionCount = 0;
let errorCount = 0;
const featureUsage: Record<string, number> = {};

export function trackAction() {
  actionCount++;
}

export function trackFeature(name: string) {
  featureUsage[name] = (featureUsage[name] || 0) + 1;
}

export function trackError() {
  errorCount++;
}

function resetCounters() {
  sessionStart = Date.now();
  actionCount = 0;
  errorCount = 0;
  Object.keys(featureUsage).forEach((k) => delete featureUsage[k]);
}

async function sendTelemetry(eventType = 'periodic', requestId: string | null = null) {
  if (!tokenManager.getToken() || !getDeviceId()) {
    resetCounters();
    return;
  }

  const now = Date.now();
  const durationSecs = Math.round((now - sessionStart) / 1000);
  const minutes = durationSecs / 60;
  const apm = minutes > 0 ? actionCount / minutes : 0;

  const payload: Record<string, unknown> = {
    sessionDurationSecs: durationSecs,
    actionsPerMin: Math.round(apm * 100) / 100,
    featureUsage: { ...featureUsage },
    errorCount,
    avgResponseTimeMs: 0,
  };

  try {
    await api.post('/api/telemetry', {
      eventType,
      requestId,
      clientTs: new Date().toISOString(),
      payload,
    });
  } catch {
    // ignore
  }

  resetCounters();
}

export function startTelemetryWorker() {
  if (timer) return;
  sessionStart = Date.now();
  timer = setInterval(() => sendTelemetry(), INTERVAL_MS);
}

export function stopTelemetryWorker() {
  if (timer) {
    clearInterval(timer);
    timer = undefined;
  }
}

export function handleTelemetryRequest(requestId: string) {
  sendTelemetry('on_demand', requestId);
}
