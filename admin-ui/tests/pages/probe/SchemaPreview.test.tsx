import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, render } from '@solidjs/testing-library';

const schema = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
    schema: (...a: unknown[]) => schema(...a),
  },
}));

async function renderPreview() {
  const { SchemaPreview } = await import('@/pages/probe/SchemaPreview');
  return render(() => <SchemaPreview />);
}

describe('SchemaPreview', () => {
  beforeEach(() => vi.clearAllMocks());

  it('加载中显示 Spinner，标题带固定 event_type', async () => {
    schema.mockReturnValue(new Promise(() => {}));
    const { container } = await renderPreview();
    await waitFor(() => expect(screen.getByRole('status')).toBeInTheDocument());
    expect(screen.getByText('数据样本 · periodic')).toBeInTheDocument();
    expect(container.querySelector('.pb-card-header .meta')).toHaveTextContent('无样本');
  });

  it('payload 为 null → Empty 兜底且 meta 显示无样本', async () => {
    schema.mockResolvedValue({ eventType: 'periodic', sampledAt: null, payload: null });
    const { container } = await renderPreview();
    await waitFor(() => expect(screen.getByText('暂无样本')).toBeInTheDocument());
    expect(schema).toHaveBeenCalledWith('periodic');
    expect(screen.getByText('telemetry_events 无 periodic 类型上报')).toBeInTheDocument();
    expect(container.querySelector('.pb-card-header .meta')).toHaveTextContent('无样本');
    expect(container.querySelector('.pb-schema')).toBeNull();
  });

  it('sampledAt 截断到秒并把 T 换成空格', async () => {
    schema.mockResolvedValue({
      eventType: 'periodic',
      sampledAt: '2026-05-29T12:34:56.789Z',
      payload: { a: 1 },
    });
    const { container } = await renderPreview();
    await waitFor(() => expect(container.querySelector('.pb-schema')).toBeTruthy());
    expect(container.querySelector('.pb-card-header .meta')).toHaveTextContent(
      '采样于 2026-05-29 12:34:56',
    );
  });

  it('对象 payload 按类型着色渲染键与各类标量', async () => {
    schema.mockResolvedValue({
      eventType: 'periodic',
      sampledAt: '2026-05-29T12:00:00Z',
      payload: {
        route: '/learn',
        clickCount: 42,
        online: true,
        lastError: null,
        nested: { model: 'iPhone' },
      },
    });
    const { container } = await renderPreview();
    await waitFor(() => expect(container.querySelector('.pb-schema')).toBeTruthy());
    const pre = container.querySelector('.pb-schema')!;
    // 键：5 个顶层 + 1 个嵌套
    expect(pre.querySelectorAll('.k')).toHaveLength(6);
    expect(Array.from(pre.querySelectorAll('.k')).map((e) => e.textContent)).toEqual([
      '"route"',
      '"clickCount"',
      '"online"',
      '"lastError"',
      '"nested"',
      '"model"',
    ]);
    // 字符串两处，布尔一处，数字 + null 各一处
    expect(Array.from(pre.querySelectorAll('.s')).map((e) => e.textContent)).toEqual([
      '"/learn"',
      '"iPhone"',
    ]);
    expect(Array.from(pre.querySelectorAll('.b')).map((e) => e.textContent)).toEqual(['true']);
    expect(Array.from(pre.querySelectorAll('.n')).map((e) => e.textContent)).toEqual([
      '42',
      'null',
    ]);
    // 逗号分隔：最后一项后不带逗号
    expect(pre.textContent).toContain('"route": "/learn",');
    expect(pre.textContent!.trimEnd().endsWith('}')).toBe(true);
  });

  it('数组 payload：逐项渲染，空数组/空对象走短路分支', async () => {
    schema.mockResolvedValue({
      eventType: 'periodic',
      sampledAt: '2026-05-29T12:00:00Z',
      payload: { list: [1, 2, 'x'], emptyArr: [], emptyObj: {} },
    });
    const { container } = await renderPreview();
    await waitFor(() => expect(container.querySelector('.pb-schema')).toBeTruthy());
    const text = container.querySelector('.pb-schema')!.textContent!;
    expect(text).toContain('"emptyArr": []');
    expect(text).toContain('"emptyObj": {}');
    expect(container.querySelectorAll('.pb-schema .n')).toHaveLength(2);
    expect(container.querySelector('.pb-schema .s')).toHaveTextContent('"x"');
  });

  it('根节点为标量数组时同样渲染（非对象 payload）', async () => {
    schema.mockResolvedValue({
      eventType: 'periodic',
      sampledAt: '2026-05-29T12:00:00Z',
      payload: ['a', 'b'],
    });
    const { container } = await renderPreview();
    await waitFor(() => expect(container.querySelector('.pb-schema')).toBeTruthy());
    expect(container.querySelectorAll('.pb-schema .s')).toHaveLength(2);
    expect(container.querySelector('.pb-schema')!.textContent).toContain('"a",');
  });
});
