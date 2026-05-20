import { createMemo } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { TextArea } from '@/components/ui/Input';

interface JsonAdvancedPanelProps {
  /** 当前 source-of-truth（已经包含未在字典中的所有字段） */
  config: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
}

/** JSON 兜底页：所有未被精雕到表单的字段在这里仍可编辑；保持与后端 295 参数全量同步。 */
export function JsonAdvancedPanel(props: JsonAdvancedPanelProps) {
  const text = createMemo(() => JSON.stringify(props.config, null, 2));

  let textareaRef: HTMLTextAreaElement | undefined;

  function applyText() {
    if (!textareaRef) return;
    try {
      const parsed = JSON.parse(textareaRef.value);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        textareaRef.setCustomValidity('需为 JSON 对象');
        textareaRef.reportValidity();
        return;
      }
      textareaRef.setCustomValidity('');
      props.onChange(parsed as Record<string, unknown>);
    } catch (e) {
      textareaRef.setCustomValidity((e as Error).message);
      textareaRef.reportValidity();
    }
  }

  return (
    <Card variant="outlined">
      <div class="flex items-baseline justify-between mb-2">
        <h3 class="text-sm font-semibold text-content">JSON 高级</h3>
        <span class="text-xs text-content-tertiary">含所有 ~295 个参数，未在表单中精雕的字段在此编辑</span>
      </div>
      <TextArea
        ref={textareaRef}
        class="h-[480px] font-mono text-xs"
        value={text()}
        spellcheck={false}
        aria-label="AMAS 配置 JSON 编辑器"
      />
      <div class="flex items-center justify-between mt-2">
        <p class="text-xs text-content-tertiary">在此修改后点「应用到表单」会同步到表单视图。保存按钮在页面顶部。</p>
        <Button size="sm" variant="outline" onClick={applyText}>应用到表单</Button>
      </div>
    </Card>
  );
}
