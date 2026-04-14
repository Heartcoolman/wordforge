import { onMount } from 'solid-js';

export function SystemLockedModal() {
  let overlayRef!: HTMLDivElement;

  onMount(() => {
    overlayRef.focus();
  });

  return (
    <div
      ref={overlayRef}
      tabIndex={0}
      class="fixed inset-0 z-[9999] flex items-center justify-center bg-black/70 outline-none"
      style="pointer-events: all"
      onClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => { e.preventDefault(); e.stopPropagation(); }}
    >
      <div class="bg-surface-elevated rounded-2xl shadow-xl p-8 max-w-sm w-full mx-4 text-center">
        <div class="flex justify-center mb-4">
          <div class="w-14 h-14 rounded-full bg-error/10 flex items-center justify-center">
            <svg class="w-8 h-8 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round"
                d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z" />
            </svg>
          </div>
        </div>
        <h2 class="text-xl font-bold text-content mb-2">数据损坏</h2>
        <p class="text-content-secondary text-sm leading-relaxed">
          客户端数据已损坏，请重启应用后再试。
        </p>
      </div>
    </div>
  );
}
