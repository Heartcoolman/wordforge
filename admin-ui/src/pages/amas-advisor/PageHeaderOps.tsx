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
      <label class="flex items-center gap-2 text-[13px] text-content-secondary select-none cursor-pointer">
        <span>启用自动巡查</span>
        <span
          role="switch"
          aria-checked={props.advisorEnabled}
          aria-label="自动巡查"
          tabindex="0"
          onClick={() => props.onToggleAutoScan(!props.advisorEnabled)}
          class={`toggle${props.advisorEnabled ? ' is-on' : ''}`}
        />
      </label>
      <button type="button" class="btn-ghost" disabled={props.running} onClick={() => props.onRunNow()}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
          <polyline points="21 3 21 8 16 8" />
        </svg>
        立即触发巡查
      </button>
      <button type="button" class="btn-llm" disabled={props.pendingCount === 0} onClick={() => props.onApproveAll()}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="20 6 9 17 4 12" />
        </svg>
        接受全部待审{props.pendingCount > 0 ? ` (${props.pendingCount})` : ''}
      </button>
    </div>
  );
}
