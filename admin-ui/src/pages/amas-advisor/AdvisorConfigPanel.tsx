import { createResource, createSignal, createEffect, Show } from 'solid-js';
import { Button } from '@/components/ui/Button';
import { Switch } from '@/components/ui/Switch';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';

export function AdvisorConfigPanel() {
  const [cfg, { refetch }] = createResource(() => adminApi.amasAdvisorConfig());
  const [saving, setSaving] = createSignal(false);

  const [monthCap, setMonthCap] = createSignal(0);
  const [autoApply, setAutoApply] = createSignal(false);
  const [maxPerDay, setMaxPerDay] = createSignal(0);
  const [minConf, setMinConf] = createSignal(0);
  const [steps, setSteps] = createSignal('20,60,100');
  const [enabled, setEnabled] = createSignal(false);
  // E3:canary 自动回滚两阈值（存于 SystemSettings，走独立 settings 端点）。
  const [canaryReward, setCanaryReward] = createSignal(0.05);
  const [canaryAnomaly, setCanaryAnomaly] = createSignal(0.05);

  createEffect(() => {
    const c = cfg();
    if (!c) return;
    setMonthCap(c.monthCapYuan);
    setAutoApply(c.autoApplyEnabled);
    setMaxPerDay(c.autoApplyMaxPerDay);
    setMinConf(c.autoApplyMinConfidence);
    setSteps(c.grayscaleSteps.join(','));
    setEnabled(c.advisorEnabled);
  });

  // canary 阈值不在 advisor config 端点，单独从 SystemSettings 读取。
  // 用 ?.() 守卫：fetcher 在 getSettings 缺失（如部分 mock 的测试）时返回 null 而非同步抛错，
  // 避免 unhandledRejection 污染共享渲染根、外溢拖垮无关测试（生产 getSettings 恒存在，行为不变）。
  const [sys] = createResource(() => adminApi.getSettings?.() ?? null);
  createEffect(() => {
    const s = sys();
    if (!s) return;
    setCanaryReward(s.canaryRewardDropThreshold);
    setCanaryAnomaly(s.canaryAnomalyRiseThreshold);
  });

  async function save() {
    const parsedSteps = steps().split(',').map((s) => parseInt(s.trim(), 10)).filter((n) => Number.isFinite(n));
    // 前端 0~1 范围校验（后端同步校验，双保险）
    if (canaryReward() < 0 || canaryReward() > 1 || canaryAnomaly() < 0 || canaryAnomaly() > 1) {
      uiStore.toast.error('保存失败', 'canary 阈值必须在 0~1 之间');
      return;
    }
    setSaving(true);
    try {
      await adminApi.amasUpdateAdvisorConfig({
        monthCapYuan: monthCap(),
        autoApplyEnabled: autoApply(),
        autoApplyMaxPerDay: maxPerDay(),
        autoApplyMinConfidence: minConf(),
        grayscaleSteps: parsedSteps.length === 3 ? (parsedSteps as [number, number, number]) : undefined,
        advisorEnabled: enabled(),
      });
      await adminApi.updateSettings({
        canaryRewardDropThreshold: canaryReward(),
        canaryAnomalyRiseThreshold: canaryAnomaly(),
      });
      uiStore.toast.success('顾问配置已保存');
      void refetch();
    } catch (e) {
      uiStore.toast.error('保存失败', e instanceof Error ? e.message : '');
    } finally {
      setSaving(false);
    }
  }

  const inputCls = 'mt-1 w-full h-9 px-3 rounded-lg text-sm bg-surface text-content border border-border-hairline font-mono focus-ring-soft focus:border-accent';

  return (
    <div class="panel">
      <div class="panel-title">顾问配置</div>
      <Show when={!cfg.error} fallback={<Empty title="配置加载失败" description={cfg.error instanceof Error ? cfg.error.message : ''} />}>
        <Show when={cfg()} fallback={<div class="flex justify-center py-8"><Spinner size="sm" /></div>}>
          {(c) => (
            <div>
              {/* 只读区：config-row */}
              <div class="config-row">
                <div class="l"><strong>模型</strong><span>DeepSeek 兼容协议，仅 leader 实例运行</span></div>
                <div class="r"><span class="chip chip-llm">{c().model}</span></div>
              </div>
              <div class="config-row">
                <div class="l"><strong>巡查频率</strong><span>cron 表达式</span></div>
                <div class="r">{c().pollCron}</div>
              </div>
              <div class="config-row">
                <div class="l"><strong>API Key</strong><span>从 .env 读取，仅显尾号</span></div>
                <div class="r">••••{c().apiKeyTail}</div>
              </div>

              {/* 可写区 */}
              <div class="border-t border-border-hairline mt-3 pt-3 space-y-3">
                <Switch checked={enabled()} onChange={setEnabled} label="启用自动巡查" />
                <Switch checked={autoApply()} onChange={setAutoApply} label="启用 auto-apply" />
                <label class="block">
                  <span class="text-xs text-content-secondary">月成本上限（¥）</span>
                  <input type="number" step="0.01" min="0" aria-label="月成本上限（¥）" value={monthCap()} onInput={(e) => setMonthCap(parseFloat(e.currentTarget.value) || 0)} class={inputCls} />
                </label>
                <label class="block">
                  <span class="text-xs text-content-secondary">auto-apply 每日上限</span>
                  <input type="number" step="1" min="0" aria-label="auto-apply 每日上限" value={maxPerDay()} onInput={(e) => setMaxPerDay(parseInt(e.currentTarget.value, 10) || 0)} class={inputCls} />
                </label>
                <label class="block">
                  <span class="text-xs text-content-secondary">auto-apply 最低置信度</span>
                  <input type="number" step="0.01" min="0" max="1" aria-label="auto-apply 最低置信度" value={minConf()} onInput={(e) => setMinConf(parseFloat(e.currentTarget.value) || 0)} class={inputCls} />
                </label>
                <label class="block">
                  <span class="text-xs text-content-secondary">灰度档位（逗号分隔，3 档）</span>
                  <input type="text" aria-label="灰度档位" value={steps()} onInput={(e) => setSteps(e.currentTarget.value)} class={inputCls} />
                </label>
                <label class="block">
                  <span class="text-xs text-content-secondary">canary 回报降幅阈值（0~1，自动回滚）</span>
                  <input type="number" step="0.01" min="0" max="1" aria-label="canary 回报降幅阈值" value={canaryReward()} onInput={(e) => setCanaryReward(parseFloat(e.currentTarget.value) || 0)} class={inputCls} />
                </label>
                <label class="block">
                  <span class="text-xs text-content-secondary">canary 异常升幅阈值（0~1，自动回滚）</span>
                  <input type="number" step="0.01" min="0" max="1" aria-label="canary 异常升幅阈值" value={canaryAnomaly()} onInput={(e) => setCanaryAnomaly(parseFloat(e.currentTarget.value) || 0)} class={inputCls} />
                </label>
                <div class="flex justify-end">
                  <Button size="sm" loading={saving()} onClick={save}>保存配置</Button>
                </div>
              </div>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
}
