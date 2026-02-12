import { Button } from '@/components/ui/Button';
import { Modal } from '@/components/ui/Modal';
import { fatigueStore } from '@/stores/fatigue';

interface FatigueWarningModalProps {
  open: boolean;
  onClose: () => void;
  /** 点击"休息一下"后的回调 */
  onRest: () => void;
}

/**
 * 严重疲劳休息建议弹窗
 * 当疲劳等级达到 severe 时弹出，建议用户休息
 */
export function FatigueWarningModal(props: FatigueWarningModalProps) {
  return (
    <Modal open={props.open} onClose={props.onClose} title="疲劳提醒" size="sm">
      <div class="text-center">
        {/* 警告图标 */}
        <div class="w-16 h-16 mx-auto mb-4 rounded-full bg-error-light flex items-center justify-center">
          <svg class="w-8 h-8 text-error" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            {/* 疲劳的脸 */}
            <circle cx="12" cy="12" r="10" />
            {/* 闭合的眼睛 */}
            <path d="M8 11c0 0 0.5-1 1.5-1s1.5 1 1.5 1" />
            <path d="M13 11c0 0 0.5-1 1.5-1s1.5 1 1.5 1" />
            {/* 嘴巴 */}
            <path d="M9 16s1-1 3-1 3 1 3 1" />
          </svg>
        </div>

        <h3 class="text-lg font-semibold text-content mb-2">你看起来有些疲劳了</h3>
        <p class="text-sm text-content-secondary mb-4">
          检测到你的疲劳分数较高，建议适当休息，保护眼睛和注意力。
        </p>

        {/* 疲劳指标卡片 */}
        <div class="bg-surface-secondary rounded-xl p-4 mb-6">
          <div class="grid grid-cols-3 gap-3 text-center">
            <div>
              <p class="text-xl font-bold text-error">{fatigueStore.fatigueScore()}</p>
              <p class="text-xs text-content-tertiary">疲劳分数</p>
            </div>
            <div>
              <p class="text-xl font-bold text-warning">{(fatigueStore.perclos() * 100).toFixed(0)}%</p>
              <p class="text-xs text-content-tertiary">闭眼比例</p>
            </div>
            <div>
              <p class="text-xl font-bold text-accent">{fatigueStore.blinkRate().toFixed(0)}</p>
              <p class="text-xs text-content-tertiary">眨眼/分钟</p>
            </div>
          </div>
        </div>

        {/* 操作按钮 */}
        <div class="flex gap-3">
          <Button
            onClick={props.onClose}
            variant="outline"
            fullWidth
          >
            继续学习
          </Button>
          <Button
            onClick={props.onRest}
            variant="primary"
            fullWidth
          >
            休息一下
          </Button>
        </div>

        <p class="text-xs text-content-tertiary mt-3">
          选择"继续学习"后 5 分钟内不会再次提醒
        </p>
      </div>
    </Modal>
  );
}
