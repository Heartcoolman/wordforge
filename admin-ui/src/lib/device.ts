const DEVICE_ID_KEY = 'eng_device_id';

export function generateUUID(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID();
  return '10000000-1000-4000-8000-100000000000'.replace(/[018]/g, (c) => {
    const n = parseInt(c);
    return (n ^ (crypto.getRandomValues(new Uint8Array(1))[0] & (15 >> (n / 4)))).toString(16);
  });
}

export function getDeviceId(): string {
  let id = localStorage.getItem(DEVICE_ID_KEY);
  if (!id) {
    id = generateUUID();
    localStorage.setItem(DEVICE_ID_KEY, id);
  }
  return id;
}

export function getDevicePlatform(): string {
  const capacitorPlatform = (globalThis as {
    Capacitor?: { getPlatform?: () => string };
  }).Capacitor?.getPlatform?.();
  if (capacitorPlatform === 'ios' || capacitorPlatform === 'android') {
    return capacitorPlatform;
  }

  const ua = navigator.userAgent;
  if (/iPhone|iPad|iPod/i.test(ua)) return 'ios';
  if (/Android/i.test(ua)) return 'android';

  // iPadOS 13+ may identify itself as macOS Safari while still exposing touch.
  if (/Macintosh/i.test(ua) && navigator.maxTouchPoints > 1) return 'ios';

  return 'web';
}

export interface DeviceFingerprint {
  cpuCores: number;
  memoryGb: number | null;
  screenWidth: number;
  screenHeight: number;
  pixelRatio: number;
  osName: string;
  browserName: string;
  browserVersion: string;
  timezone: string;
  language: string;
  touchSupport: boolean;
  onlineStatus: boolean;
}

function parseUserAgent(): { osName: string; browserName: string; browserVersion: string } {
  const ua = navigator.userAgent;
  let osName = 'Unknown';
  let browserName = 'Unknown';
  let browserVersion = '';

  if (/iPhone|iPad|iPod/.test(ua)) {
    const m = ua.match(/OS (\d+[._]\d+)/);
    osName = 'iOS ' + (m ? m[1].replace('_', '.') : '');
  } else if (/Mac OS X/.test(ua)) {
    const m = ua.match(/Mac OS X (\d+[._]\d+)/);
    osName = 'macOS ' + (m ? m[1].replace('_', '.') : '');
  } else if (/Android/.test(ua)) {
    const m = ua.match(/Android (\d+[\.\d]*)/);
    osName = 'Android ' + (m ? m[1] : '');
  } else if (/Windows/.test(ua)) {
    const m = ua.match(/Windows NT (\d+\.\d+)/);
    osName = 'Windows ' + (m ? m[1] : '');
  } else if (/Linux/.test(ua)) {
    osName = 'Linux';
  }

  if (/Edg\//.test(ua)) {
    browserName = 'Edge';
    const m = ua.match(/Edg\/([\d.]+)/);
    browserVersion = m ? m[1] : '';
  } else if (/OPR\//.test(ua)) {
    browserName = 'Opera';
    const m = ua.match(/OPR\/([\d.]+)/);
    browserVersion = m ? m[1] : '';
  } else if (/Chrome\//.test(ua)) {
    browserName = 'Chrome';
    const m = ua.match(/Chrome\/([\d.]+)/);
    browserVersion = m ? m[1] : '';
  } else if (/Firefox\//.test(ua)) {
    browserName = 'Firefox';
    const m = ua.match(/Firefox\/([\d.]+)/);
    browserVersion = m ? m[1] : '';
  } else if (/Safari\//.test(ua)) {
    browserName = 'Safari';
    const m = ua.match(/Version\/([\d.]+)/);
    browserVersion = m ? m[1] : '';
  }

  return { osName, browserName, browserVersion };
}

export function collectDeviceFingerprint(): DeviceFingerprint {
  const { osName, browserName, browserVersion } = parseUserAgent();
  return {
    cpuCores: navigator.hardwareConcurrency ?? 1,
    memoryGb: (navigator as any).deviceMemory ?? null,
    screenWidth: screen.width,
    screenHeight: screen.height,
    pixelRatio: devicePixelRatio,
    osName,
    browserName,
    browserVersion,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    language: navigator.language,
    touchSupport: navigator.maxTouchPoints > 0,
    onlineStatus: navigator.onLine,
  };
}
