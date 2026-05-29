import { Button } from '@/components/ui/Button';

export interface PageHeaderOpsProps {
  advisorEnabled: boolean;
  running: boolean;
  pendingCount: number;
  onToggleAutoScan: (next: boolean) => void;
  onRunNow: () => void;
  onApproveAll: () => void;
}

export function PageHeaderOps(props: PageHeaderOpsProps) {
  return (
    <div class="flex flex-wrap items-center gap-3">
      <button
        type="button"
        role="switch"
        aria-checked={props.advisorEnabled}
        aria-label="自动巡查"
        onClick={() => props.onToggleAutoScan(!props.advisorEnabled)}
        class={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
          props.advisorEnabled ? 'bg-accent' : 'bg-surface-secondary border border-border-hairline'
        }`}
      >
        <span class="text-[11px] absolute -top-4 left-0 whitespace-nowrap text-content-tertiary">自动巡查</span>
        <span class={`inline-block size-4 transform rounded-full bg-white transition-transform ${
          props.advisorEnabled ? 'translate-x-6' : 'translate-x-1'
        }`} />
      </button>
      <Button size="sm" variant="outline" loading={props.running} onClick={() => props.onRunNow()}>
        立即触发巡查
      </Button>
      <Button size="sm" disabled={props.pendingCount === 0} onClick={() => props.onApproveAll()}>
        接受全部待审{props.pendingCount > 0 ? ` (${props.pendingCount})` : ''}
      </Button>
    </div>
  );
}
