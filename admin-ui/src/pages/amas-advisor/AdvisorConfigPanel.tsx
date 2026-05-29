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

  async function save() {
    const parsedSteps = steps().split(',').map((s) => parseInt(s.trim(), 10)).filter((n) => Number.isFinite(n));
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
