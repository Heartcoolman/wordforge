import { createSignal, Show, createMemo, createEffect, on, onMount } from 'solid-js';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { ConfigTree } from '@/components/amas/ConfigTree';
import { DiffSummary } from '@/components/amas/DiffSummary';
import { InlineVersionList } from '@/components/amas/InlineVersionList';
import { CanaryCard } from './CanaryCard';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import type { AmasConfig } from '@/types/amas';
import type { FieldError } from './schema';
import TomlEditor from '@/components/amas/TomlEditor';
import { EditorView } from '@codemirror/view';

interface JsonAdvancedPanelProps {
  /** source of truth(由父 AmasConfigPage 维护) */
  config: Record<string, unknown>;
  /** 服务端 baseline,用于 DiffSummary */
  baseline: Record<string, unknown>;
  errors: FieldError[];
  onChange: (next: Record<string, unknown>) => void;
  /** 点击右栏"最近 10 次"或外层"版本历史"按钮 */
  onOpenVersionDrawer: () => void;
  /** refresh trigger(父在 save 后改变此值 → InlineVersionList refetch) */
  versionRefreshKey?: () => unknown;
}

/**
 * JSON 高级 pane(对齐 amas-config.html JSON pane 三栏 IDE 布局):
 *
 * ┌───────┬────────────────────────┬──────────┐
 * │ Tree  │  TOML editor + toolbar │ Canary    │
 * │ Tree  │  + DiffSummary         │ Version   │
 * └───────┴────────────────────────┴──────────┘
 *
 * - 左栏 ConfigTree:section 锚点 → 触发 TomlEditor 命令式 scrollIntoView + 光标定位
 * - 中栏 TomlEditor(CodeMirror 6 + StreamLanguage/TOML legacy-mode):
 *   • config 变化时拉 serialize-toml(draft 中保护用户输入,不覆盖)
 *   • 应用 ⌘S → parse-toml → onChange 同步回父
 *   • 工具栏: 行数 / 状态 chip / 重载 baseline / 应用
 * - 中栏底部 DiffSummary:vs baseline 的 N 处改动表
 * - 右栏 CanaryCard + InlineVersionList(最近 10 版本 + 跳转 Drawer)
 *
 * 响应式: <lg 单列堆叠, lg 双列(树+中), xl 三列。
 */
export function JsonAdvancedPanel(props: JsonAdvancedPanelProps) {
  const [activeSection, setActiveSection] = createSignal<string>('');
  const [tomlText, setTomlText] = createSignal('');
  const [tomlDraft, setTomlDraft] = createSignal<string | null>(null);
  const [tomlBusy, setTomlBusy] = createSignal(false);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);
  let editorView: EditorView | undefined;

  // mount 时初次序列化 + 跟随 config 变化(draft 中不覆盖)
  onMount(() => {
    void refreshTomlFromConfig();
  });

  createEffect(
    on(
      () => props.config,
      async (cfg) => {
        if (tomlDraft() !== null) return; // 用户编辑中,不覆盖
        await refreshTomlFromConfig(cfg);
      },
      { defer: true },
    ),
  );

  async function refreshTomlFromConfig(cfg = props.config) {
    setTomlBusy(true);
    try {
      const r = await adminApi.amasSerializeToml(cfg as unknown as AmasConfig);
      setTomlText(r.toml);
      setTomlDraft(null);
      setErrorMsg(null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : 'TOML 序列化失败';
      uiStore.toast.error('TOML 序列化失败', msg);
    } finally {
      setTomlBusy(false);
    }
  }

  async function applyToml() {
    const raw = tomlDraft();
    if (raw === null) return;
    setTomlBusy(true);
    try {
      const parsed = await adminApi.amasParseToml(raw);
      props.onChange(parsed as unknown as Record<string, unknown>);
      setTomlText(raw);
      setTomlDraft(null);
      setErrorMsg(null);
      uiStore.toast.success('TOML 已应用到配置(未保存)');
    } catch (e) {
      const msg = e instanceof Error ? e.message : '解析失败';
      setErrorMsg(msg);
    } finally {
      setTomlBusy(false);
    }
  }

  function discardDraft() {
    setTomlDraft(null);
    setErrorMsg(null);
    void refreshTomlFromConfig();
  }

  /** ConfigTree 节点点击 → 在 TOML 内 search section 锚点并 scrollIntoView */
  function handleSectionClick(section: string) {
    setActiveSection(section);
    if (!editorView) return;
    const text = editorView.state.doc.toString();
    // 优先 [xxx.section] 严格匹配,fallback 到任意位置
    const candidates = [
      `[${section}]`,
      `[engine.${section}]`,
      `[algorithms.${section}]`,
      `[scoring.${section}]`,
      `[limits.${section}]`,
      section,
    ];
    let pos = -1;
    for (const c of candidates) {
      const idx = text.indexOf(c);
      if (idx >= 0) {
        pos = idx;
        break;
      }
    }
    if (pos < 0) return;
    editorView.dispatch({
      selection: { anchor: pos, head: pos },
      effects: EditorView.scrollIntoView(pos, { y: 'center' }),
    });
    editorView.focus();
  }

  const currentText = createMemo(() => tomlDraft() ?? tomlText());
  const isDirty = () => tomlDraft() !== null;
  const lineCount = createMemo(() => currentText().split('\n').length);

  return (
    <div class="grid grid-cols-1 lg:grid-cols-[260px_minmax(0,1fr)] xl:grid-cols-[260px_minmax(0,1fr)_340px] gap-4 items-start">
      {/* LEFT: ConfigTree */}
      <ConfigTree activeSection={activeSection()} onSectionClick={handleSectionClick} />

      {/* CENTER: toolbar + editor + diff */}
      <div class="space-y-3 min-w-0">
        {/* ac-toolbar(对齐设计图深色 IDE 工具栏,这里浅色版本) */}
        <div class="flex flex-wrap items-center gap-2 rounded-md bg-surface-secondary px-3 py-2 text-xs">
          <span class="font-mono text-content-tertiary tabular-nums">
            amas_config.toml · UTF-8 · {lineCount()} 行
          </span>
          <Show when={isDirty()}>
            <Badge variant="warning" size="sm">
              已编辑
            </Badge>
          </Show>
          <Show when={!isDirty() && !tomlBusy()}>
            <span class="inline-flex items-center gap-1.5 text-success-strong">
              <span class="size-1.5 rounded-full bg-success animate-pulse" />
              热加载就绪 · 防抖 500ms
            </span>
          </Show>
          <Show when={tomlBusy()}>
            <span class="text-content-tertiary">同步中…</span>
          </Show>
          <div class="ml-auto flex flex-wrap gap-1.5">
            <Button size="sm" variant="ghost" onClick={discardDraft} disabled={!isDirty()}>
              丢弃草稿
            </Button>
            <Button
              size="sm"
              onClick={applyToml}
              disabled={!isDirty() || tomlBusy()}
              loading={tomlBusy()}
            >
              应用 ⌘S
            </Button>
          </div>
        </div>

        {/* CodeMirror TOML */}
        <TomlEditor
          value={currentText()}
          onChange={(v) => {
            setTomlDraft(v);
            setErrorMsg(null);
          }}
          minHeightPx={480}
          onEditorReady={(view) => {
            editorView = view;
          }}
        />

        <Show when={errorMsg()}>
          <div
            class="rounded bg-error-light px-3 py-2 text-xs text-error-strong"
            role="alert"
          >
            {errorMsg()}
          </div>
        </Show>

        {/* DiffSummary vs baseline */}
        <DiffSummary
          baseline={props.baseline}
          config={props.config}
          errors={props.errors}
        />
      </div>

      {/* RIGHT: CanaryCard + InlineVersionList */}
      <div class="space-y-3 xl:sticky xl:top-24">
        <CanaryCard />
        <InlineVersionList
          onOpenFullList={props.onOpenVersionDrawer}
          refreshKey={props.versionRefreshKey}
        />
      </div>
    </div>
  );
}
