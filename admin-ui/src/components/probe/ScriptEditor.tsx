/**
 * CodeMirror 6 包装：admin 探针 script 编辑器（JS mode）。
 * 受控组件：父组件传 value + onChange，内部状态由 CodeMirror 管理。
 *
 * 已知限制：
 * - dark mode 切换不会响应（theme 仅在 onMount 编译一次）；与 Group F EChart theme 同期处理 → DEFERRED
 * - 圆角/边框由外层 div 控制（避免 CodeMirror theme 与外层容器双重圆角）
 */

import { onMount, onCleanup, createEffect } from 'solid-js';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, highlightActiveLine } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { javascript } from '@codemirror/lang-javascript';

interface Props {
  value: string;
  onChange: (value: string) => void;
  minHeightPx?: number;
}

// 用户连续输入期间的反向同步窗口；窗口内忽略 props.value → editor 的覆盖，
// 避免父级 normalize（trim / autoformat）回写时打断光标 / 选区
const USER_EDIT_DEBOUNCE_MS = 500;

export default function ScriptEditor(props: Props) {
  let container: HTMLDivElement | undefined;
  let view: EditorView | undefined;
  let userEditingTimer: number | undefined;
  let userEditing = false;

  onMount(() => {
    if (!container) return;
    const minH = `${props.minHeightPx ?? 180}px`;
    const state = EditorState.create({
      doc: props.value,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        javascript(),
        EditorView.lineWrapping,
        EditorView.theme({
          // 注意：边框 / 圆角让外层 div 控制，CodeMirror 内只管字号、背景、最小高度
          '&': {
            fontSize: '12px',
            backgroundColor: 'rgb(var(--color-surface-secondary, 250 250 250) / 1)',
            minHeight: minH,
          },
          '.cm-content': { fontFamily: 'ui-monospace, SFMono-Regular, monospace' },
          '&.cm-focused': { outline: 'none' },
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) {
            const next = u.state.doc.toString();
            if (next !== props.value) {
              userEditing = true;
              if (userEditingTimer !== undefined) window.clearTimeout(userEditingTimer);
              userEditingTimer = window.setTimeout(() => {
                userEditing = false;
              }, USER_EDIT_DEBOUNCE_MS);
              props.onChange(next);
            }
          }
        }),
      ],
    });
    view = new EditorView({ state, parent: container });
  });

  // 父组件通过 setValue 强制覆盖（如模板回填）→ 同步到 editor；
  // 用户键入期间的 props 变化（onChange → parent → props.value）不再回写，避免光标跳动
  createEffect(() => {
    const v = props.value;
    if (!view) return;
    if (userEditing) return;
    if (view.state.doc.toString() !== v) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: v },
      });
    }
  });

  onCleanup(() => {
    if (userEditingTimer !== undefined) window.clearTimeout(userEditingTimer);
    view?.destroy();
    view = undefined;
  });

  return (
    <div
      ref={container}
      class="rounded border border-border-hairline overflow-hidden"
      style={{ 'min-height': `${props.minHeightPx ?? 180}px` }}
    />
  );
}
