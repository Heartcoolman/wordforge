import { createSignal, Show } from 'solid-js';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { IconAlert, IconDryRun, IconWarnTri } from './icons';

interface Props {
  active: boolean;
  lastMaintenance?: string;
  onChange: (active: boolean) => void;
}

export function MaintenanceBanner(props: Props) {
  const [showConfirm, setShowConfirm] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [dryRun, setDryRun] = createSignal<string | null>(null);

  async function apply(next: boolean) {
    setBusy(true);
    try {
      await adminApi.setMaintenance(next);
      props.onChange(next);
      uiStore.toast.success(next ? '维护模式已开启' : '维护模式已关闭');
    } catch (e) {
      uiStore.toast.error('切换失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  }

  function toggle() {
    if (!props.active) { setShowConfirm(true); return; }
    void apply(false);
  }

  // 预演：纯本地推演影响面，不调用后端（无对应端点，诚实降级为前端说明）
  function runDry() {
    setDryRun(
      props.active
        ? '预演：关闭维护模式后所有非 admin 请求恢复正常路由，cron / worker 立即恢复调度。'
        : '预演：开启维护模式后所有非 admin 角色请求将被路由到维护页（HTTP 503 + Retry-After），后台 cron / worker 暂停。admin token 持有者不受影响。',
    );
  }

  return (
    <>
      <ConfirmDialog
        open={showConfirm()}
        title="确认开启维护模式"
        message="开启后所有非管理员请求将被路由到维护页，后台 cron / worker 自动暂停。确定继续吗？"
        confirmText="进入维护模式"
        variant="warning"
        onConfirm={() => { setShowConfirm(false); void apply(true); }}
        onCancel={() => setShowConfirm(false)}
      />
      <div class={`st-maint ${props.active ? 'is-on' : ''}`}>
        <div class="pict"><IconAlert /></div>
        <div style={{ flex: 1 }}>
          <h3>{props.active ? '维护模式运行中 · 仅管理员可访问' : '系统运行中 · 维护模式已关闭'}</h3>
          <p>开启维护模式后，所有非 admin 角色的请求会被路由到自定义维护页，后台 cron / worker 自动暂停。建议提前在客户端推送预告。</p>
          <div class="row-controls">
            <button class="st-btn st-btn-secondary st-btn-sm" onClick={runDry}><IconDryRun /> 预演 (dry-run)</button>
            <button class="st-btn st-btn-secondary st-btn-sm btn-warn" disabled={busy()} onClick={toggle}>
              <IconWarnTri /> {props.active ? '退出维护模式' : '进入维护模式'}
            </button>
            <Show when={props.lastMaintenance}>
              <span class="last">上次维护 · {props.lastMaintenance}</span>
            </Show>
          </div>
          <Show when={dryRun()}>
            <p class="helper" style={{ 'margin-top': '10px', 'max-width': '70ch' }}>{dryRun()}</p>
          </Show>
        </div>
      </div>
    </>
  );
}
