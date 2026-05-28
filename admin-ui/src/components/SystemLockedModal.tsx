import { Modal } from '@/components/ui/Modal';
import { Button } from '@/components/ui/Button';

/**
 * 系统锁定屏（数据损坏等场景）。
 * - hideClose + onClose no-op：禁用 ESC / backdrop 关闭，避免误操作隐藏致命错误
 * - 提供"刷新页面"按钮作为唯一出口，对键盘用户也可达（Modal 已实现 focus trap）
 * - 与 MaintenancePage 互斥分发需在调用方协调（App.tsx 范围外）；本组件保持 z-50，
 *   MaintenancePage z-9999 在更上层，并发时维护页视觉覆盖，可接受。
 */
export function SystemLockedModal() {
  return (
    <Modal open={true} onClose={() => {}} size="sm" hideClose>
      <div class="text-center">
        <div class="flex justify-center mb-4">
          <div class="w-14 h-14 rounded-full bg-error/10 flex items-center justify-center">
            <svg class="w-8 h-8 text-error" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" aria-hidden="true">
              <path stroke-linecap="round" stroke-linejoin="round"
                d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z" />
            </svg>
          </div>
        </div>
        <h2 class="text-xl font-bold text-content mb-2">数据损坏</h2>
        <p class="text-content-secondary text-sm leading-relaxed mb-5">
          客户端数据已损坏，请重启应用后再试。
        </p>
        <div class="flex justify-center">
          <Button variant="primary" onClick={() => window.location.reload()}>刷新页面</Button>
        </div>
      </div>
    </Modal>
  );
}
