import { createMemo, createSignal, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import type { AmasConfig } from '@/types/amas';
import { cn } from '@/utils/cn';

interface JsonAdvancedPanelProps {
  /** 当前 source-of-truth（已经包含未在字典中的所有字段） */
  config: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
}

type Lang = 'json' | 'toml';

/** JSON / TOML 兜底页：所有未被精雕到表单的字段在这里仍可编辑；保持与后端 295 参数全量同步。 */
export function JsonAdvancedPanel(props: JsonAdvancedPanelProps) {
  const [lang, setLang] = createSignal<Lang>('json');
  // m022:tomlText 缓存,避免每次切回 TOML 都打后端
  const [tomlText, setTomlText] = createSignal<string>('');
  const [tomlBusy, setTomlBusy] = createSignal(false);
  // 跟随模式（draft 为 null）：textarea 显示 stringify(config)；编辑模式：显示 draft()
  const followText = createMemo(() => JSON.stringify(props.config, null, 2));
  const [draft, setDraft] = createSignal<string | null>(null);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

  let textareaRef: HTMLTextAreaElement | undefined;

  // 切换到 TOML:用后端 serialize-toml 把当前 config 转 TOML 字符串
  async function switchToToml() {
    if (lang() === 'toml') return;
    setTomlBusy(true);
    try {
      const r = await adminApi.amasSerializeToml(props.config as unknown as AmasConfig);
      setTomlText(r.toml);
      setLang('toml');
      setDraft(null);
      setErrorMsg(null);
    } catch (e) {
      uiStore.toast.error('TOML 序列化失败', e instanceof Error ? e.message : '');
    } finally {
      setTomlBusy(false);
    }
  }

  function switchToJson() {
    if (lang() === 'json') return;
    setLang('json');
    setDraft(null);
    setErrorMsg(null);
  }

  async function applyText() {
    if (lang() === 'toml') {
      const raw = draft() ?? tomlText();
      setTomlBusy(true);
      try {
        const parsed = await adminApi.amasParseToml(raw);
        props.onChange(parsed as unknown as Record<string, unknown>);
        setTomlText(raw);
        setDraft(null);
        setErrorMsg(null);
        uiStore.toast.success('TOML 已应用到表单');
      } catch (e) {
        const msg = e instanceof Error ? e.message : '解析失败';
        setErrorMsg(msg);
      } finally {
        setTomlBusy(false);
      }
      return;
    }
    const raw = draft() ?? followText();
    try {
      const parsed = JSON.parse(raw);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        setErrorMsg('需为 JSON 对象');
        textareaRef?.setCustomValidity('需为 JSON 对象');
        textareaRef?.reportValidity();
        return;
      }
      setErrorMsg(null);
      textareaRef?.setCustomValidity('');
      props.onChange(parsed as Record<string, unknown>);
      setDraft(null); // 应用后回到跟随模式
    } catch (e) {
      const msg = (e as Error).message;
      setErrorMsg(msg);
      textareaRef?.setCustomValidity(msg);
      textareaRef?.reportValidity();
    }
  }

  function resetDraft() {
    setDraft(null);
    setErrorMsg(null);
    textareaRef?.setCustomValidity('');
  }

  const isDirty = () => draft() !== null;
  const currentText = () => draft() ?? (lang() === 'toml' ? tomlText() : followText());

  return (
    <Card variant="outlined">
      <div class="flex items-baseline justify-between mb-2 gap-3 flex-wrap">
        <div>
          <h3 class="text-sm font-semibold text-content">原始配置编辑</h3>
          <p class="text-xs text-content-tertiary mt-0.5">含所有 ~295 个参数,未在表单中精雕的字段在此编辑</p>
        </div>
        {/* m022:JSON/TOML toggle */}
        <div
          class="inline-flex rounded-md border border-border-hairline overflow-hidden text-xs"
          role="group"
          aria-label="编辑器语言"
        >
          <button
            type="button"
            class={cn(
              'px-3 py-1 transition-colors duration-fast',
              lang() === 'json' ? 'bg-accent text-accent-content' : 'bg-surface text-content-secondary hover:bg-surface-secondary',
            )}
            onClick={switchToJson}
            aria-pressed={lang() === 'json'}
          >
            JSON
          </button>
          <button
            type="button"
            class={cn(
              'px-3 py-1 transition-colors duration-fast border-l border-border-hairline',
              lang() === 'toml' ? 'bg-accent text-accent-content' : 'bg-surface text-content-secondary hover:bg-surface-secondary',
              tomlBusy() && 'opacity-50 cursor-wait',
            )}
            onClick={switchToToml}
            aria-pressed={lang() === 'toml'}
            disabled={tomlBusy()}
          >
            TOML
          </button>
        </div>
      </div>
      {/* 使用裸 textarea 避免 TextArea wrapper 的 ref 透传不确定性，并支持非受控编辑（光标不跳） */}
      <textarea
        ref={textareaRef}
        class={cn(
          'w-full h-[480px] px-3 py-2 rounded-lg font-mono text-xs bg-surface text-content',
          'border border-border-hairline transition-[border-color,box-shadow,background-color] duration-fast ease-out-expo',
          'placeholder:text-content-tertiary hover:border-border',
          'focus-ring-soft focus:border-accent resize-y',
          errorMsg() && 'border-error focus:border-error',
        )}
        value={currentText()}
        onInput={(e) => {
          setDraft(e.currentTarget.value);
          if (errorMsg()) {
            setErrorMsg(null);
            e.currentTarget.setCustomValidity('');
          }
        }}
        spellcheck={false}
        aria-label={`AMAS 配置 ${lang().toUpperCase()} 编辑器`}
        aria-invalid={errorMsg() ? true : undefined}
      />
      <Show when={errorMsg()}>
        <p class="text-xs text-error mt-1" role="alert">{errorMsg()}</p>
      </Show>
      <div class="flex items-center justify-between mt-2 gap-2 flex-wrap">
        <p class="text-xs text-content-tertiary">
          {isDirty() ? `已编辑 (${lang().toUpperCase()}),未应用` : `在此修改后点「应用到表单」会经后端 ${lang() === 'toml' ? '解析' : '校验'}后同步。保存按钮在页面顶部。`}
        </p>
        <div class="flex items-center gap-2">
          <Show when={isDirty()}>
            <Button size="sm" variant="ghost" onClick={resetDraft}>重置</Button>
          </Show>
          <Button size="sm" variant="outline" onClick={applyText} disabled={!isDirty()} loading={tomlBusy()}>应用到表单</Button>
        </div>
      </div>
    </Card>
  );
}
