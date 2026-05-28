import { FATIGUE_WARNING_COOLDOWN_MS } from './constants';

const SESSION_KEY = 'lastFatigueWarningTime';

let lastWarningTime = Number(sessionStorage.getItem(SESSION_KEY) || '0');

export function checkFatigueWarningCooldown(): boolean {
  const now = Date.now();
  if (now - lastWarningTime > FATIGUE_WARNING_COOLDOWN_MS) {
    lastWarningTime = now;
    sessionStorage.setItem(SESSION_KEY, String(now));
    return true;
  }
  return false;
}
