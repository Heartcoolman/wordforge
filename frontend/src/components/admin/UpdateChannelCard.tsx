import { Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import type { ChannelStatus } from '@/types/admin';

/**
 * 单通道升级卡片。Stable / Beta 两通道复用此组件，区别仅在 channel + status 来源。
 *
 * - 按钮 disabled 规则：!hasUpdate || !canApply || applying
 * - hasUpdate=false 时显示「已是最新」灰按钮；canApply=false 但 hasUpdate=true 时
 *   显示警告（架构不匹配的 release）
 * - Release Notes 内嵌为 `<details>`，默认折叠，与卡片本身的 collapsible 区分
 */
interface Props {
  channel: 'stable' | 'beta';
  status: ChannelStatus | null;
  applying: boolean;
  onApply: () => void;
}

const CHANNEL_LABEL: Record<'stable' | 'beta', string> = {
  stable: '稳定通道',
  beta: 'Beta 通道',
};

export function UpdateChannelCard(props: Props) {
  const status = () => props.status;
  const disabled = () =>
    !status()?.hasUpdate || !status()?.canApply || props.applying;

  return (
    <Card>
      <div class="flex items-start justify-between gap-4 mb-3">
        <div>
          <p class="text-caption text-content-secondary mb-1">
            {CHANNEL_LABEL[props.channel]} · 远端最新
          </p>
          <p class="text-2xl font-semibold text-content">
            {status()?.latestVersion ?? '尚未检查'}
          </p>
        </div>
        <Button
          disabled={disabled()}
          loading={props.applying}
          onClick={props.onApply}
        >
          {status()?.hasUpdate
            ? `一键升级到 ${status()!.latestVersion}`
            : '已是最新'}
        </Button>
      </div>

      <Show when={status()?.hasUpdate && !status()?.canApply}>
        <p class="text-sm text-warning mb-3">
          远端发布了新版本，但 <strong>未找到匹配当前架构的产物</strong>。
          请检查 release.yml 是否覆盖此平台。
        </p>
      </Show>

      <Show when={status()?.releaseNotes}>
        <details class="mt-2">
          <summary class="text-sm text-accent cursor-pointer select-none">
            Release Notes
          </summary>
          <pre class="mt-2 whitespace-pre-wrap text-xs text-content-secondary font-mono leading-relaxed max-h-64 overflow-y-auto bg-surface-secondary/40 p-2 rounded">
            {status()!.releaseNotes}
          </pre>
          <Show when={status()?.releaseUrl}>
            <a
              href={status()!.releaseUrl}
              target="_blank"
              rel="noopener"
              class="text-xs text-accent hover:underline mt-1 inline-block"
            >
              在 GitHub 打开 ↗
            </a>
          </Show>
        </details>
      </Show>
    </Card>
  );
}
