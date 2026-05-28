import { Show } from 'solid-js';
import { updateInfo, setUpdateInfo } from '@/api/http';

export function UpdateBanner() {
  return (
    <Show when={updateInfo()}>
      {(info) => (
        <div
          role="alert"
          aria-live="polite"
          // z-50 区分 toast（z-100），fixed top-0 仍需 AdminLayout 配合 padding-top（Group G）兜底
          class="fixed top-0 left-0 right-0 z-50 bg-accent/10 backdrop-blur-md border-b border-accent/30 text-accent px-4 py-3 flex items-center justify-between shadow-elevation-1 animate-slide-down"
        >
          <span class="text-sm font-medium">{info().message}</span>
          <div class="flex items-center gap-3">
            <button
              type="button"
              onClick={() => window.location.reload()}
              class="text-sm font-semibold hover:underline transition-opacity duration-fast"
            >
              刷新
            </button>
            <button
              type="button"
              onClick={() => setUpdateInfo(null)}
              class="text-sm font-semibold opacity-80 hover:opacity-100 transition-opacity duration-fast"
            >
              关闭
            </button>
          </div>
        </div>
      )}
    </Show>
  );
}
