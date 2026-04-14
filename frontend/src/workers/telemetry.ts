import { api } from '@/api/client';
import { collectDeviceFingerprint, getDeviceId } from '@/lib/device';
import { tokenManager } from '@/lib/token';

const INTERVAL_MS = 5000;

let timer: ReturnType<typeof setInterval> | undefined;
let sessionStart = Date.now();

// Legacy counters (kept for backwards-compat fields)
let actionCount = 0;
let errorCount = 0;
const featureUsage: Record<string, number> = {};

// Behavior counters (active buffer)
interface BehaviorBuffer {
  currentRoute: string;
  clickCount: number;
  clickTargets: Array<{ label: string; tag: string }>;
  scrollDepthPct: number;
  visibilityChanges: number;
  routeChanges: number;
}

function newBuffer(): BehaviorBuffer {
  return {
    currentRoute: window.location.pathname,
    clickCount: 0,
    clickTargets: [],
    scrollDepthPct: 0,
    visibilityChanges: 0,
    routeChanges: 0,
  };
}

let behavior: BehaviorBuffer = newBuffer();

function getInteractiveLabel(el: Element): { label: string; tag: string } {
  const tag = el.tagName.toLowerCase();
  const label =
    el.getAttribute('aria-label') ||
    el.getAttribute('title') ||
    ((el as HTMLElement).innerText || '').slice(0, 50) ||
    '';
  return { label, tag };
}

function findInteractiveAncestor(target: EventTarget | null): Element | null {
  if (!(target instanceof Element)) return null;
  const tags = new Set(['button', 'a', 'input', 'select', 'textarea']);
  let el: Element | null = target;
  while (el) {
    if (tags.has(el.tagName.toLowerCase()) || el.hasAttribute('role')) return el;
    el = el.parentElement;
  }
  return target as Element;
}

function onClickCapture(e: MouseEvent) {
  behavior.clickCount++;
  if (behavior.clickTargets.length < 20) {
    const anchor = findInteractiveAncestor(e.target);
    if (anchor) behavior.clickTargets.push(getInteractiveLabel(anchor));
  }
}

function onScroll() {
  const scrolled = window.scrollY + window.innerHeight;
  const total = document.documentElement.scrollHeight;
  if (total > 0) {
    const pct = (scrolled / total) * 100;
    if (pct > behavior.scrollDepthPct) behavior.scrollDepthPct = pct;
  }
}

function onVisibilityChange() {
  behavior.visibilityChanges++;
}

function onPopState() {
  behavior.scrollDepthPct = 0;
  behavior.routeChanges++;
  behavior.currentRoute = window.location.pathname;
}

export function trackAction() {
  actionCount++;
}

export function trackFeature(name: string) {
  featureUsage[name] = (featureUsage[name] || 0) + 1;
}

export function trackError() {
  errorCount++;
}

function resetLegacyCounters() {
  sessionStart = Date.now();
  actionCount = 0;
  errorCount = 0;
  Object.keys(featureUsage).forEach((k) => delete featureUsage[k]);
}

async function sendTelemetry(
  eventType = 'periodic',
  requestId: string | null = null,
  includeDevice = false,
) {
  if (!tokenManager.getToken() || !getDeviceId()) {
    resetLegacyCounters();
    return;
  }

  const now = Date.now();
  const durationSecs = Math.round((now - sessionStart) / 1000);
  const minutes = durationSecs / 60;
  const apm = minutes > 0 ? actionCount / minutes : 0;

  // Buffer swap: snapshot and replace before HTTP roundtrip
  const snapshot = behavior;
  behavior = newBuffer();

  const payload: Record<string, unknown> = {
    sessionDurationSecs: durationSecs,
    actionsPerMin: Math.round(apm * 100) / 100,
    featureUsage: { ...featureUsage },
    errorCount,
    avgResponseTimeMs: 0,
    behavior: snapshot,
  };

  if (includeDevice) {
    payload.device = collectDeviceFingerprint();
  }

  try {
    await api.post('/api/telemetry', {
      eventType,
      requestId,
      clientTs: new Date().toISOString(),
      payload,
    });
    resetLegacyCounters();
  } catch {
    // On failure: merge snapshot back into current buffer to avoid data loss
    behavior.clickCount += snapshot.clickCount;
    behavior.visibilityChanges += snapshot.visibilityChanges;
    behavior.routeChanges += snapshot.routeChanges;
    if (snapshot.scrollDepthPct > behavior.scrollDepthPct) {
      behavior.scrollDepthPct = snapshot.scrollDepthPct;
    }
    // Prepend snapshot (older) before current buffer (newer), cap at 20
    behavior.clickTargets = [...snapshot.clickTargets, ...behavior.clickTargets].slice(0, 20);
  }
}

export function startTelemetryWorker() {
  if (timer) return;
  sessionStart = Date.now();
  behavior = newBuffer();

  document.addEventListener('click', onClickCapture, { capture: true, passive: true });
  window.addEventListener('scroll', onScroll, { passive: true });
  document.addEventListener('visibilitychange', onVisibilityChange);
  window.addEventListener('popstate', onPopState);

  // Patch history.pushState to capture SPA navigations
  const origPushState = history.pushState.bind(history);
  (history as any)._telemetryPushState = origPushState;
  history.pushState = function (...args) {
    behavior.scrollDepthPct = 0;
    behavior.routeChanges++;
    origPushState(...args);
    behavior.currentRoute = window.location.pathname;
  };

  // Send session_start immediately with device fingerprint
  sendTelemetry('session_start', null, true);

  timer = setInterval(() => sendTelemetry(), INTERVAL_MS);
}

export function stopTelemetryWorker() {
  if (!timer) return;
  clearInterval(timer);
  timer = undefined;
  document.removeEventListener('click', onClickCapture, { capture: true });
  window.removeEventListener('scroll', onScroll);
  document.removeEventListener('visibilitychange', onVisibilityChange);
  window.removeEventListener('popstate', onPopState);
  if ((history as any)._telemetryPushState) {
    history.pushState = (history as any)._telemetryPushState;
    delete (history as any)._telemetryPushState;
  }
}

export function handleTelemetryRequest(requestId: string) {
  sendTelemetry('on_demand', requestId);
}
