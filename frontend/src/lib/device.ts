const DEVICE_ID_KEY = 'eng_device_id';

function generateUUID(): string {
  return crypto.randomUUID();
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
  return 'web';
}
