import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@solidjs/testing-library';

// JsonAdvancedPanel 重构后接入 CodeMirror + adminApi.amasSerializeToml/ParseToml,
// CodeMirror 在 happy-dom 下渲染不稳,这里只 smoke + 验证工具栏渲染,
// 真实 TOML 编辑 / parse 由 e2e 覆盖。

// mock CanaryCard 避免拉真实 canary endpoint
vi.mock('@/pages/amas/CanaryCard', () => ({
  CanaryCard: () => null,
}));

// mock TomlEditor 避免 CodeMirror 在 happy-dom 下 DOM 操作
vi.mock('@/components/amas/TomlEditor', () => ({
  default: () => null,
}));

// mock InlineVersionList 避免 amasListVersions fetch
vi.mock('@/components/amas/InlineVersionList', () => ({
  InlineVersionList: () => null,
}));

// mock adminApi(serialize / parse)+ 让 onMount 异步序列化能完成
vi.mock('@/api/admin', () => ({
  adminApi: {
    amasSerializeToml: vi.fn().mockResolvedValue({ toml: '# initial\n' }),
    amasParseToml: vi.fn().mockResolvedValue({ b: 2 }),
    amasListVersions: vi.fn().mockResolvedValue([]),
  },
}));

import { JsonAdvancedPanel } from '@/pages/amas/JsonAdvancedPanel';

describe('JsonAdvancedPanel(三栏 IDE 重构)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders three-column layout with ConfigTree + toolbar + DiffSummary', () => {
    render(() => (
      <JsonAdvancedPanel
        config={{ a: 1 }}
        baseline={{ a: 1 }}
        errors={[]}
        onChange={() => {}}
        onOpenVersionDrawer={() => {}}
      />
    ));
    // 左栏:ConfigTree header
    expect(screen.getByText('配置树')).toBeTruthy();
    // 工具栏 + 配置树底部 path 都含 amas_config.toml,getAllByText 兼容多份
    expect(screen.getAllByText(/amas_config\.toml/).length).toBeGreaterThan(0);
    // DiffSummary 标题(无 dirty 时显示空提示)
    expect(screen.getByText(/修改摘要/)).toBeTruthy();
  });

  it('toolbar 应用按钮在无 draft 时 disabled', () => {
    render(() => (
      <JsonAdvancedPanel
        config={{ a: 1 }}
        baseline={{ a: 1 }}
        errors={[]}
        onChange={() => {}}
        onOpenVersionDrawer={() => {}}
      />
    ));
    const applyBtn = screen.getByText(/应用 ⌘S/).closest('button') as HTMLButtonElement;
    expect(applyBtn.disabled).toBe(true);
  });

  it('config 与 baseline 一致时 DiffSummary 显示空状态', () => {
    render(() => (
      <JsonAdvancedPanel
        config={{ a: 1 }}
        baseline={{ a: 1 }}
        errors={[]}
        onChange={() => {}}
        onOpenVersionDrawer={() => {}}
      />
    ));
    expect(screen.getByText(/无修改|完全一致/)).toBeTruthy();
  });
});
