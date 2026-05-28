import { For } from 'solid-js';
import { PARAM_DICT } from '@/pages/amas/schema';
import { cn } from '@/utils/cn';

interface TreeNode {
  /** schema section key,用于回调 + 高亮 */
  section: string;
  /** 显示名 */
  label: string;
}

interface TreeGroup {
  label: string;
  children: TreeNode[];
}

/** 4 大顶层分组(对齐 amas-config.html .tree-panel 设计图布局,
 *  schema 真实 8 section 按语义归并). */
const TREE_GROUPS: TreeGroup[] = [
  {
    label: 'engine',
    children: [
      { section: 'featureFlags', label: 'featureFlags' },
      { section: 'coldStart', label: 'coldStart' },
    ],
  },
  {
    label: 'algorithms',
    children: [
      { section: 'ensemble', label: 'ensemble' },
      { section: 'memoryModel', label: 'memoryModel (FSRS-5)' },
    ],
  },
  {
    label: 'scoring',
    children: [
      { section: 'modeling', label: 'modeling' },
      { section: 'objectiveWeights', label: 'objectiveWeights' },
      { section: 'constraints', label: 'constraints' },
    ],
  },
  {
    label: 'limits',
    children: [{ section: 'monitoring', label: 'monitoring' }],
  },
];

interface ConfigTreeProps {
  /** 当前高亮的 section(由父决定) */
  activeSection?: string;
  /** 点击 section 节点回调(父负责切 pane + anchor jump) */
  onSectionClick: (section: string) => void;
}

/**
 * 配置树(JSON pane 左栏 sticky):
 * - 4 大顶层分组 engine / algorithms / scoring / limits
 * - 子节点点击触发 onSectionClick(section),父决定行为(切 pane / anchor jump)
 * - 底部固定显示 amas_config.toml 路径
 *
 * 对齐 amas-config.html .tree-panel 设计图;不参与配置读写,仅导航。
 */
export function ConfigTree(props: ConfigTreeProps) {
  const totalSections = Object.keys(PARAM_DICT).length;

  return (
    <aside class="rounded-lg bg-surface-elevated p-4 shadow-elevation-1 sticky top-24 max-h-[calc(100vh-7rem)] overflow-y-auto">
      <div class="flex items-baseline justify-between mb-3">
        <h4 class="text-sm font-semibold text-content">配置树</h4>
        <span class="text-[11px] font-mono text-content-tertiary tabular-nums">
          {totalSections} sections
        </span>
      </div>

      <ul class="space-y-2">
        <For each={TREE_GROUPS}>
          {(group) => (
            <li>
              <div class="flex items-center gap-1.5 text-[12px] font-medium text-content py-1">
                <svg
                  class="size-3 text-content-tertiary"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  aria-hidden="true"
                >
                  <path d="M6 9l6 6 6-6" />
                </svg>
                <span>{group.label}</span>
              </div>
              <ul class="ml-3 space-y-px border-l border-border-hairline pl-2">
                <For each={group.children}>
                  {(child) => (
                    <li>
                      <button
                        type="button"
                        class={cn(
                          'w-full text-left text-[12px] py-1 px-2 rounded-sm font-mono',
                          'transition-colors duration-fast hover:bg-surface-secondary',
                          props.activeSection === child.section &&
                            'bg-accent-light/40 text-accent font-medium',
                        )}
                        onClick={() => props.onSectionClick(child.section)}
                      >
                        {child.label}
                      </button>
                    </li>
                  )}
                </For>
              </ul>
            </li>
          )}
        </For>
      </ul>

      <div class="mt-4 pt-3 border-t border-border-hairline">
        <div class="text-[10px] uppercase tracking-wide text-content-tertiary mb-1">
          配置文件路径
        </div>
        <code class="text-[10.5px] font-mono text-accent break-all leading-snug block">
          /etc/wordforge/amas_config.toml
        </code>
      </div>
    </aside>
  );
}
