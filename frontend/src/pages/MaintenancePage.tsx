import { onMount, onCleanup } from 'solid-js';

// 维护页是最高层级，z-9999 是与 Modal/Toast/Banner 体系协调后的最顶层（design token 之外的显式约定）。
const MAINTENANCE_POLL_INTERVAL_MS = 5_000;

export default function MaintenancePage() {
  onMount(() => {
    // 文案承诺"自动恢复"，需要轮询 /api/system/health；恢复后整页 reload 让 SPA 重新进入正常路由。
    let timer: number | undefined;
    let aborted = false;

    const tick = async () => {
      if (aborted) return;
      try {
        const r = await fetch('/api/system/health', { cache: 'no-store' });
        if (r.ok) {
          aborted = true;
          window.location.reload();
          return;
        }
      } catch {
        /* 网络错误时继续轮询 */
      }
      timer = window.setTimeout(tick, MAINTENANCE_POLL_INTERVAL_MS);
    };

    timer = window.setTimeout(tick, MAINTENANCE_POLL_INTERVAL_MS);

    onCleanup(() => {
      aborted = true;
      if (timer !== undefined) window.clearTimeout(timer);
    });
  });

  return (
    <div class="fixed inset-0 z-[9999] flex flex-col items-center justify-center bg-surface" role="status" aria-live="polite">
      <svg
        class="w-16 h-16 text-content-tertiary mb-6 animate-[spin_4s_linear_infinite]"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        aria-hidden="true"
      >
        <path d="M10.343 3.94c.09-.542.56-.94 1.11-.94h1.093c.55 0 1.02.398 1.11.94l.149.894c.07.424.384.764.78.93s.844.083 1.168-.166l.715-.544a1.12 1.12 0 0 1 1.543.155l.773.773a1.12 1.12 0 0 1 .155 1.543l-.544.715c-.249.324-.305.78-.166 1.168s.506.71.93.78l.894.15c.542.09.94.56.94 1.109v1.094c0 .55-.398 1.02-.94 1.11l-.894.149c-.424.07-.764.383-.93.78s-.083.844.166 1.168l.544.715a1.12 1.12 0 0 1-.155 1.543l-.773.773a1.12 1.12 0 0 1-1.543.155l-.715-.544c-.324-.249-.78-.305-1.168-.166s-.71.505-.78.929l-.15.894c-.09.542-.56.94-1.109.94h-1.094c-.55 0-1.02-.398-1.11-.94l-.148-.894c-.071-.424-.384-.764-.781-.93s-.844-.083-1.168.166l-.715.544a1.12 1.12 0 0 1-1.543-.155l-.773-.773a1.12 1.12 0 0 1-.155-1.543l.544-.715c.249-.324.305-.78.166-1.168s-.506-.71-.93-.78l-.894-.15c-.542-.09-.94-.56-.94-1.109v-1.094c0-.55.398-1.02.94-1.11l.894-.148c.424-.071.764-.384.93-.781s.083-.844-.166-1.168l-.544-.715a1.12 1.12 0 0 1 .155-1.543l.773-.773a1.12 1.12 0 0 1 1.543-.155l.715.544c.324.249.78.305 1.168.166s.71-.506.78-.93l.15-.894z" />
        <path d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0z" />
      </svg>
      <h1 class="text-2xl font-bold text-content mb-2">服务器维护中</h1>
      <p class="text-content-secondary text-center max-w-sm">
        系统正在进行维护升级，请稍后再试。维护结束后页面将自动恢复。
      </p>
    </div>
  );
}
