/**
 * CodeMirror 6 包装：admin 探针 script 编辑器（JS mode）。
 * 受控组件：父组件传 value + onChange，内部状态由 CodeMirror 管理。
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

export default function ScriptEditor(props: Props) {
  let container: HTMLDivElement | undefined;
  let view: EditorView | undefined;

  onMount(() => {
    if (!container) return;
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
          '&': {
            fontSize: '12px',
            backgroundColor: 'rgb(var(--color-surface-secondary, 250 250 250) / 1)',
            border: '1px solid rgb(var(--color-border-hairline, 230 230 230) / 1)',
            borderRadius: '4px',
            minHeight: `${props.minHeightPx ?? 180}px`,
          },
          '.cm-content': { fontFamily: 'ui-monospace, SFMono-Regular, monospace' },
          '&.cm-focused': { outline: 'none' },
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) {
            const next = u.state.doc.toString();
            if (next !== props.value) props.onChange(next);
          }
        }),
      ],
    });
    view = new EditorView({ state, parent: container });
  });

  // 父组件通过 setValue 强制覆盖（比如模板回填）→ 同步到 editor
  createEffect(() => {
    const v = props.value;
    if (!view) return;
    if (view.state.doc.toString() !== v) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: v },
      });
    }
  });

  onCleanup(() => {
    view?.destroy();
    view = undefined;
  });

  return <div ref={container} class="rounded" />;
}
