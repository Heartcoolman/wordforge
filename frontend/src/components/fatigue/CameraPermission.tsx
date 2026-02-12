import { Button } from '@/components/ui/Button';
import { Modal } from '@/components/ui/Modal';

interface CameraPermissionProps {
  open: boolean;
  onClose: () => void;
  /** 用户确认开启摄像头时的回调 */
  onConfirm: () => void;
}

/**
 * 摄像头权限引导弹窗
 * 解释疲劳检测需要摄像头的原因，以及隐私保护声明
 */
export function CameraPermission(props: CameraPermissionProps) {
  return (
    <Modal open={props.open} onClose={props.onClose} title="开启疲劳检测" size="sm">
      <div class="text-center">
        {/* 摄像头 + 隐私盾牌图标 */}
        <div class="flex items-center justify-center gap-3 mb-5">
          {/* 摄像头图标 */}
          <div class="w-14 h-14 rounded-full bg-accent/10 flex items-center justify-center">
            <svg class="w-7 h-7 text-accent" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M23 7l-7 5 7 5V7z" />
              <rect x="1" y="5" width="15" height="14" rx="2" ry="2" />
            </svg>
          </div>
          {/* 隐私盾牌图标 */}
          <div class="w-14 h-14 rounded-full bg-success-light flex items-center justify-center">
            <svg class="w-7 h-7 text-success" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
              <path d="M9 12l2 2 4-4" />
            </svg>
          </div>
        </div>

        <h3 class="text-lg font-semibold text-content mb-2">需要使用摄像头</h3>
        <p class="text-sm text-content-secondary mb-4">
          疲劳检测通过分析面部特征来判断你的疲劳状态，帮助你在学习时合理休息。
        </p>

        {/* 隐私说明 */}
        <div class="bg-surface-secondary rounded-xl p-4 mb-6 text-left">
          <h4 class="text-sm font-medium text-content mb-2 flex items-center gap-2">
            <svg class="w-4 h-4 text-success" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            </svg>
            隐私保护
          </h4>
          <ul class="text-xs text-content-secondary space-y-1.5">
            <li class="flex items-start gap-2">
              <span class="text-success mt-0.5">&#10003;</span>
              所有检测在本地完成，视频不会上传到服务器
            </li>
            <li class="flex items-start gap-2">
              <span class="text-success mt-0.5">&#10003;</span>
              仅上报疲劳分数数值，不上传视频或图像
            </li>
            <li class="flex items-start gap-2">
              <span class="text-success mt-0.5">&#10003;</span>
              随时可以关闭检测功能
            </li>
          </ul>
        </div>

        {/* 操作按钮 */}
        <div class="flex gap-3">
          <Button onClick={props.onClose} variant="outline" fullWidth>
            取消
          </Button>
          <Button onClick={props.onConfirm} variant="primary" fullWidth>
            开启摄像头
          </Button>
        </div>
      </div>
    </Modal>
  );
}
