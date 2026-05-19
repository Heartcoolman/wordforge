import { Modal } from '@/components/ui/Modal';

/** 系统锁定屏（数据损坏等场景）。无关闭按钮、点击/ESC 不可解除，需重启应用。 */
export function SystemLockedModal() {
  return (
    <Modal open={true} onClose={() => {}} size="sm" hideClose>
      <div class="text-center">
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
    </Modal>
  );
}
