import { createResource, createSignal, Show, For } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Input } from '@/components/ui/Input';
import { Spinner } from '@/components/ui/Spinner';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';

/**
 * m022:AMAS 灰度发布卡片。
 *
 * 当前的 active canary 通过 GET /admin/amas/config/canary 获取;
 * set canary 需选目标 version_hash + 百分比 + 强制用户 ID 列表;
 * disable 走 POST /admin/amas/config/canary/disable。
 *
 * 引擎按 user_id hash % 100 < percent 命中即用 canary 配置(force_user_ids 必中),否则走 stable。
 */
export function CanaryCard() {
  const [canaryResp, { refetch }] = createResource(() => adminApi.amasGetCanary());
  const canary = () => canaryResp()?.canary ?? null;
  const [versions] = createResource(() => adminApi.amasListVersions(50));

  const [draftVersion, setDraftVersion] = createSignal('');
  const [draftPercent, setDraftPercent] = createSignal(10);
  const [draftForceIds, setDraftForceIds] = createSignal('');
  const [saving, setSaving] = createSignal(false);
  const [confirmDisable, setConfirmDisable] = createSignal(false);

  async function setCanary() {
    const v = draftVersion().trim();
    if (!v) {
      uiStore.toast.error('请选择目标版本');
      return;
    }
    const p = Math.max(0, Math.min(100, Math.round(draftPercent())));
    const forceUserIds = draftForceIds()
      .split(/[\s,，]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    setSaving(true);
    try {
      await adminApi.amasSetCanary({ versionHash: v, percent: p, forceUserIds });
      uiStore.toast.success(`灰度已启用 · ${p}% + ${forceUserIds.length} 个强制用户`);
      await refetch();
    } catch (e) {
      uiStore.toast.error('灰度设置失败', e instanceof Error ? e.message : '');
    } finally {
      setSaving(false);
    }
  }

  async function disableCanary() {
    setConfirmDisable(false);
    setSaving(true);
    try {
      const r = await adminApi.amasDisableCanary();
      uiStore.toast.success(r.disabled ? '灰度已停用' : '当前无 active canary');
      await refetch();
    } catch (e) {
      uiStore.toast.error('停用失败', e instanceof Error ? e.message : '');
    } finally {
      setSaving(false);
    }
  }

  return (
    <Card variant="elevated">
      <div class="flex items-baseline justify-between mb-3 gap-3 flex-wrap">
        <div>
          <h2 class="text-headline text-content">灰度发布(Canary)</h2>
          <p class="text-xs text-content-tertiary mt-0.5">
            按 user_id hash 百分比 + 强制用户 ID 抽样,引擎热路径每事件按用户分流读取
          </p>
        </div>
        <Show when={canary()}>
          {(c) => (
            <Badge variant={c() ? 'warning' : 'default'} size="sm" dot>
              {c() ? `生效中 · ${c()!.percent}%` : '未启用'}
            </Badge>
          )}
        </Show>
      </div>

      {/* 当前 active 状态 */}
      <Show when={!canaryResp.loading} fallback={<Spinner size="sm" />}>
        <Show
          when={canary()}
          fallback={
            <div class="p-3 rounded-md bg-surface-secondary text-xs text-content-secondary">
              暂无生效中的灰度配置 · 所有用户走 stable 通道
            </div>
          }
        >
          {(c) => (
            <div class="p-3 rounded-md bg-warning/10 border border-warning/30 space-y-1.5 text-xs">
              <div class="flex flex-wrap gap-x-4 gap-y-1">
                <span class="text-content-secondary">
                  目标版本 · <span class="font-mono text-content">{c()!.versionHash.slice(0, 12)}</span>
                </span>
                <span class="text-content-secondary">
                  百分比 · <span class="font-mono text-content tabular-nums">{c()!.percent}%</span>
                </span>
                <span class="text-content-secondary">
                  强制用户 · <span class="font-mono text-content tabular-nums">{c()!.forceUserIds.length}</span>
                </span>
                <span class="text-content-secondary">
                  创建 · <span class="font-mono text-content-tertiary">{new Date(c()!.createdAt).toLocaleString('zh-CN', { hour12: false })}</span>
                </span>
              </div>
              <Show when={c()!.forceUserIds.length > 0}>
                <div class="text-content-tertiary font-mono break-all">
                  IDs · {c()!.forceUserIds.slice(0, 5).join(', ')}
                  {c()!.forceUserIds.length > 5 && ` +${c()!.forceUserIds.length - 5}`}
                </div>
              </Show>
              <div class="pt-1">
                <Button size="sm" variant="ghost" onClick={() => setConfirmDisable(true)} loading={saving()}>
                  停用灰度
                </Button>
              </div>
            </div>
          )}
        </Show>
      </Show>

      {/* 设置新灰度 */}
      <div class="mt-4 pt-4 border-t border-border-hairline space-y-3">
        <h3 class="text-sm font-semibold text-content">设置新灰度</h3>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <div>
            <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">目标版本</p>
            <select
              value={draftVersion()}
              onChange={(e) => setDraftVersion(e.currentTarget.value)}
              class="w-full px-3 py-2 rounded-md border border-border-hairline bg-surface text-content text-sm focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/30"
            >
              <option value="">— 选择已存在的 version_hash —</option>
              <Show when={versions()}>
                <For each={versions()!.slice(0, 50)}>
                  {(v) => (
                    <option value={v.versionHash}>
                      {v.versionHash.slice(0, 10)} · {new Date(v.createdAt).toLocaleString('zh-CN', { hour12: false })}
                    </option>
                  )}
                </For>
              </Show>
            </select>
          </div>
          <div>
            <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">
              百分比 ({draftPercent()}%)
            </p>
            <input
              type="range"
              min={0}
              max={100}
              step={5}
              value={draftPercent()}
              onInput={(e) => setDraftPercent(parseInt(e.currentTarget.value, 10))}
              class="w-full accent-accent"
              aria-label="灰度百分比"
            />
          </div>
        </div>
        <div>
          <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">
            强制命中的用户 ID(可选,逗号或换行分隔)
          </p>
          <Input
            placeholder="如 user_001, user_002 或留空"
            value={draftForceIds()}
            onInput={(e) => setDraftForceIds(e.currentTarget.value)}
          />
          <p class="text-[10.5px] text-content-tertiary mt-1">
            列在此处的用户 100% 命中 canary,不受百分比限制 · 适合内部测试账号
          </p>
        </div>
        <div class="flex justify-end">
          <Button size="sm" onClick={setCanary} loading={saving()}>启用 / 替换灰度</Button>
        </div>
      </div>

      <ConfirmDialog
        open={confirmDisable()}
        title="确认停用灰度"
        message="停用后所有用户立即回退到 stable 通道,不影响 stable 配置本身。"
        confirmText="确认停用"
        variant="warning"
        onConfirm={disableCanary}
        onCancel={() => setConfirmDisable(false)}
      />
    </Card>
  );
}
