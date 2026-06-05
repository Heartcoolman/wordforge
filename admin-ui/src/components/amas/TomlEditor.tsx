/**
 * CodeMirror 6 TOML 编辑器(StreamLanguage + legacy-modes/toml)。
 *
 * 与 ScriptEditor 同一架构(line numbers / history / active-line),
 * 仅把 javascript() 替换为 StreamLanguage.define(toml)。
 */

import { onMount, onCleanup, createEffect } from 'solid-js';
import { EditorState } from '@codemirror/state';
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLineGutter,
  highlightActiveLine,
} from '@codemirror/view';
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from '@codemirror/commands';
import { StreamLanguage } from '@codemirror/language';
import { toml } from '@codemirror/legacy-modes/mode/toml';

interface Props {
  value: string;
  onChange: (value: string) => void;
  minHeightPx?: number;
  /** 暴露 view ref,供父调 scrollPosIntoView 等命令式 API */
  onEditorReady?: (view: EditorView) => void;
}

// 用户连续输入时反向同步窗口,避免父级 normalize 回写打断光标
const USER_EDIT_DEBOUNCE_MS = 500;

export default function TomlEditor(props: Props) {
  let container: HTMLDivElement | undefined;
  let view: EditorView | undefined;
  let userEditingTimer: number | undefined;
  let userEditing = false;

  onMount(() => {
    if (!container) return;
    const minH = `${props.minHeightPx ?? 400}px`;
    const state = EditorState.create({
      doc: props.value,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        StreamLanguage.define(toml),
        EditorView.lineWrapping,
        EditorView.theme({
          '&': {
            fontSize: '13px',
            minHeight: minH,
            backgroundColor: 'var(--color-surface, #fff)',
          },
          '.cm-content': {
            fontFamily: 'ui-monospace, SFMono-Regular, "JetBrains Mono", monospace',
            padding: '12px 0',
          },
          '.cm-gutters': {
            backgroundColor: 'var(--color-surface-secondary, #f6f6f6)',
            borderRight: '1px solid var(--color-border-hairline, #e5e5e5)',
          },
          '.cm-activeLineGutter': {
            backgroundColor: 'transparent',
            color: 'var(--accent, #6366f1)',
          },
          '&.cm-focused': { outline: 'none' },
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) {
            const next = u.state.doc.toString();
            if (next !== props.value) {
              userEditing = true;
              if (userEditingTimer !== undefined)
                window.clearTimeout(userEditingTimer);
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
    props.onEditorReady?.(view);
  });

  // 父组件强制覆盖(如重载 baseline)→ 同步到 editor;
  // 用户键入期间的 props.value 不写,避免光标跳动
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
      class="rounded-lg border border-border-hairline overflow-hidden bg-surface"
      style={{ 'min-height': `${props.minHeightPx ?? 400}px` }}
    />
  );
}
