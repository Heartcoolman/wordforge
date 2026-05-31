import { createResource, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Switch } from '@/components/ui/Switch';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import type { SamplingRule } from '@/types/probeTelemetry';
import { uiStore } from '@/stores/ui';
import { LockIcon, ratePct } from './util';

/** 采样规则卡：真实 telemetry event_type 规则，按 priority 升序。
 *  per-row 行内编辑：可调行改采样率(0~100%)、所有行可暂停/启用，写 PATCH /sampling/:event_type。
 *  locked 行（核心数据强制）禁止改采样率（后端 409），仅允许暂停/启用。
 *  「新增规则」：后端无 create 端点 —— 规则集随迁移预置，故按钮禁用并标注，不伪造能力。 */
export function SamplingRulesPanel() {
  const [data, { refetch }] = createResource(() => probeTelemetryApi.sampling());
  const [editing, setEditing] = createSignal<string | null>(null);
  const [draft, setDraft] = createSignal(0); // 编辑中的百分比
  const [saving, setSaving] = createSignal(false);

  const beginEdit = (r: SamplingRule) => {
    setEditing(r.eventType);
    setDraft(ratePct(r.sampleRate));
  };
  const cancel = () => setEditing(null);

  // 保存采样率（仅可调行）
  const saveRate = (r: SamplingRule) => {
    const next = draft();
    if (next === ratePct(r.sampleRate)) { cancel(); return; }
    setSaving(true);
    probeTelemetryApi
      .updateSamplingRule(r.eventType, { sampleRate: next / 100 })
      .then(() => { uiStore.toast.success('采样率已更新', `${r.eventType} → ${next}%`); cancel(); return refetch(); })
      .catch((err: unknown) => uiStore.toast.error('更新失败', err instanceof Error ? err.message : '请求失败'))
      .finally(() => setSaving(false));
  };

  // 暂停 / 启用（含 locked 行）
  const toggleEnabled = (r: SamplingRule, enabled: boolean) => {
    probeTelemetryApi
      .updateSamplingRule(r.eventType, { enabled })
      .then(() => { uiStore.toast.success(enabled ? '规则已启用' : '规则已暂停', r.eventType); return refetch(); })
      .catch((err: unknown) => uiStore.toast.error('操作失败', err instanceof Error ? err.message : '请求失败'));
  };

  return (
    <Card variant="elevated" padding="md">
      <div class="pb-card-header">
        <div>
          <h3>采样级联规则</h3>
          <span class="meta">
            <Show when={data()} fallback="加载中">
              {(d) => `从上到下命中即停 · 未命中走全局默认 ${ratePct(d().globalDefault)}%`}
            </Show>
          </span>
        </div>
        {/* 新增规则：后端无 create 端点，规则随迁移预置 → 禁用并标注，避免伪造能力 */}
        <button
          class="btn btn-secondary"
          style={{ 'font-size': '12px', padding: '6px 12px' }}
          disabled
          title="规则集随数据库迁移预置，当前仅支持编辑/暂停已有规则"
        >
          + 新增规则
        </button>
      </div>
      <Show
        when={!data.loading}
        fallback={<div class="min-h-[120px] grid place-items-center"><Spinner /></div>}
      >
        <Show
          when={(data()?.rules.length ?? 0) > 0}
          fallback={<Empty title="暂无采样规则" description="probe_sampling_config 表为空" />}
        >
          <div>
            <For each={data()!.rules}>
              {(rule) => {
                const off = () => !rule.enabled || rule.sampleRate === 0;
                const isEditing = () => editing() === rule.eventType;
                return (
                  <div class={`pb-rule${off() ? ' is-off' : ''}`}>
                    <div><code>{rule.eventType}</code></div>
                    <div class="target">
                      {rule.target ? `绑定探针 ${rule.target}` : '—'}
                      {rule.locked ? ' · 核心数据强制' : ''}
                      {!rule.enabled ? ' · 已暂停' : ''}
                    </div>
                    <Show
                      when={isEditing()}
                      fallback={
                        <div class="pct-tag" style={off() ? { color: 'var(--error-strong)' } : undefined}>
                          {ratePct(rule.sampleRate)}%
                        </div>
                      }
                    >
                      <div class="pb-rule-edit">
                        <input
                          type="number"
                          min="0"
                          max="100"
                          value={draft()}
                          disabled={rule.locked}
                          aria-label={`${rule.eventType} 采样率`}
                          onInput={(e) => setDraft(Math.max(0, Math.min(100, Number(e.currentTarget.value) || 0)))}
                        />
                        <span class="suffix">%</span>
                      </div>
                    </Show>
                    <div class="rule-flags">
                      <Show
                        when={isEditing()}
                        fallback={
                          <div class="pb-rule-actions">
                            <span title={rule.enabled ? '点击暂停采集' : '点击恢复采集'}>
                              <Switch
                                checked={rule.enabled}
                                onChange={(v: boolean) => toggleEnabled(rule, v)}
                              />
                            </span>
                            <Show
                              when={!rule.locked}
                              fallback={<span class="lock-badge" title="采样率锁定（核心数据）"><LockIcon /></span>}
                            >
                              <button class="pb-rule-btn" title="编辑采样率" onClick={() => beginEdit(rule)}>编辑</button>
                            </Show>
                          </div>
                        }
                      >
                        <div class="pb-rule-actions">
                          <button class="pb-rule-btn" disabled={saving()} onClick={() => saveRate(rule)}>保存</button>
                          <button class="pb-rule-btn is-ghost" disabled={saving()} onClick={cancel}>取消</button>
                        </div>
                      </Show>
                    </div>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>
      </Show>
    </Card>
  );
}
