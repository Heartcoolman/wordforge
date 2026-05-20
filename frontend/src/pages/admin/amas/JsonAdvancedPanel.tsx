import { createMemo, createSignal, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { cn } from '@/utils/cn';

interface JsonAdvancedPanelProps {
  /** 当前 source-of-truth（已经包含未在字典中的所有字段） */
  config: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
}

/** JSON 兜底页：所有未被精雕到表单的字段在这里仍可编辑；保持与后端 295 参数全量同步。 */
export function JsonAdvancedPanel(props: JsonAdvancedPanelProps) {
  // 跟随模式（draft 为 null）：textarea 显示 stringify(config)；编辑模式：显示 draft()
  const followText = createMemo(() => JSON.stringify(props.config, null, 2));
  const [draft, setDraft] = createSignal<string | null>(null);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

  let textareaRef: HTMLTextAreaElement | undefined;

  function applyText() {
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

  return (
    <Card variant="outlined">
      <div class="flex items-baseline justify-between mb-2">
        <h3 class="text-sm font-semibold text-content">JSON 高级</h3>
        <span class="text-xs text-content-tertiary">含所有 ~295 个参数，未在表单中精雕的字段在此编辑</span>
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
        value={draft() ?? followText()}
        onInput={(e) => {
          setDraft(e.currentTarget.value);
          if (errorMsg()) {
            setErrorMsg(null);
            e.currentTarget.setCustomValidity('');
          }
        }}
        spellcheck={false}
        aria-label="AMAS 配置 JSON 编辑器"
        aria-invalid={errorMsg() ? true : undefined}
      />
      <Show when={errorMsg()}>
        <p class="text-xs text-error mt-1" role="alert">{errorMsg()}</p>
      </Show>
      <div class="flex items-center justify-between mt-2 gap-2 flex-wrap">
        <p class="text-xs text-content-tertiary">
          {isDirty() ? '已编辑，未应用' : '在此修改后点「应用到表单」会同步到表单视图。保存按钮在页面顶部。'}
        </p>
        <div class="flex items-center gap-2">
          <Show when={isDirty()}>
            <Button size="sm" variant="ghost" onClick={resetDraft}>重置</Button>
          </Show>
          <Button size="sm" variant="outline" onClick={applyText} disabled={!isDirty()}>应用到表单</Button>
        </div>
      </div>
    </Card>
  );
}
