import { createSignal, onCleanup } from 'solid-js';
import { uiStore } from '@/stores/ui';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import type { ProbeRow } from '@/types/probeTelemetry';
import { ratePct } from './util';

/** 采样滑杆。locked 探针（learn/perf，无 samplingEventType）disabled 灰化；
 *  可调探针（click，samplingEventType="periodic"）拖动后 debounce 300ms PATCH
 *  对应 telemetry event_type，成功 toast，失败回滚并 toast。 */
export function SamplingSlider(props: { probe: ProbeRow; onSaved?: () => void }) {
  const locked = () => props.probe.locked || !props.probe.samplingEventType;
  const [pct, setPct] = createSignal(ratePct(props.probe.sampleRate));
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => { if (timer) clearTimeout(timer); });

  const commit = (next: number) => {
    const et = props.probe.samplingEventType;
    if (!et) return;
    const prev = ratePct(props.probe.sampleRate);
    probeTelemetryApi
      .updateSamplingRule(et, { sampleRate: next / 100 })
      .then(() => {
        uiStore.toast.success('采样率已更新', `${props.probe.label} → ${next}%`);
        props.onSaved?.();
      })
      .catch((err: unknown) => {
        setPct(prev);
        const msg = err instanceof Error ? err.message : '请求失败';
        uiStore.toast.error('采样率更新失败', msg);
      });
  };

  const onInput = (e: InputEvent & { currentTarget: HTMLInputElement }) => {
    const v = Number(e.currentTarget.value);
    setPct(v);
    if (locked()) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => commit(v), 300);
  };

  return (
    <div class={`pb-sample${locked() ? ' is-locked' : ''}`}>
      <input
        type="range"
        min="0"
        max="100"
        value={pct()}
        disabled={locked()}
        aria-label={`${props.probe.label} 采样率`}
        onInput={onInput}
      />
      <span class="pct">{pct()}%</span>
    </div>
  );
}
