import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, render } from '@solidjs/testing-library';
import type { SinkStatus } from '@/types/probeTelemetry';

const sinks = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
    sinks: (...a: unknown[]) => sinks(...a),
  },
}));

function mkSink(over: Partial<SinkStatus> = {}): SinkStatus {
  return {
    id: 'telemetry_events',
    label: '遥测事件',
    kind: 'table',
    rowCount: 12345,
    lastWriteTs: '2026-05-29T11:59:00Z',
    retentionDays: null,
    lagSecs: 42,
    ...over,
  };
}

async function renderPanel() {
  const { SinksPanel } = await import('@/pages/probe/SinksPanel');
  return render(() => <SinksPanel />);
}

describe('SinksPanel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('加载中显示 Spinner 与"加载中"计数', async () => {
    sinks.mockReturnValue(new Promise(() => {}));
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByRole('status')).toBeInTheDocument());
    expect(container.querySelector('.pb-card-header .meta')).toHaveTextContent('加载中');
    expect(container.querySelector('.pb-sink')).toBeNull();
  });

  it('sinks 为空 → Empty 兜底且计数为 0', async () => {
    sinks.mockResolvedValue({ generatedAt: '2026-05-29T12:00:00Z', sinks: [] });
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('暂无 sink')).toBeInTheDocument());
    expect(screen.getByText('sinks_status 返回空')).toBeInTheDocument();
    expect(container.querySelector('.pb-card-header .meta')).toHaveTextContent('0 个存储位置');
  });

  it('渲染每张表：首字母缩写、行数缩写、永久保留、延迟秒数', async () => {
    sinks.mockResolvedValue({
      generatedAt: '2026-05-29T12:00:00Z',
      sinks: [mkSink(), mkSink({ id: 'learning_records', label: '学习记录', rowCount: 500 })],
    });
    const { container } = await renderPanel();
    await waitFor(() => expect(container.querySelectorAll('.pb-sink')).toHaveLength(2));
    expect(container.querySelector('.pb-card-header .meta')).toHaveTextContent('2 个存储位置');

    const rows = container.querySelectorAll('.pb-sink');
    expect(rows[0].querySelector('.logo')).toHaveTextContent('TE');
    expect(rows[0].querySelector('strong')).toHaveTextContent('遥测事件');
    expect(rows[0].querySelector('strong')).toHaveAttribute('title', '技术表名：telemetry_events');
    expect(rows[0].querySelector('.meta')).toHaveTextContent(
      'telemetry_events · 12.3k 行 · 保留 永久/不限',
    );
    expect(rows[0].querySelector('.lag')).toHaveTextContent('+42s');
    expect(rows[1].querySelector('.logo')).toHaveTextContent('LE');
    expect(rows[1].querySelector('.meta')).toHaveTextContent('learning_records · 500 行');
  });

  it('retentionDays 非空 → 显示 Nd', async () => {
    sinks.mockResolvedValue({
      generatedAt: '2026-05-29T12:00:00Z',
      sinks: [mkSink({ retentionDays: 30 })],
    });
    const { container } = await renderPanel();
    await waitFor(() => expect(container.querySelector('.pb-sink')).toBeTruthy());
    expect(container.querySelector('.pb-sink .meta')).toHaveTextContent('保留 30d');
  });

  it('lagSecs > 300 标 is-late；≤ 300 不标', async () => {
    sinks.mockResolvedValue({
      generatedAt: '2026-05-29T12:00:00Z',
      sinks: [mkSink({ lagSecs: 301 }), mkSink({ id: 'sessions', label: '会话', lagSecs: 300 })],
    });
    const { container } = await renderPanel();
    await waitFor(() => expect(container.querySelectorAll('.pb-sink')).toHaveLength(2));
    const rows = container.querySelectorAll('.pb-sink');
    expect(rows[0]).toHaveClass('is-late');
    expect(rows[1]).not.toHaveClass('is-late');
  });

  it('lastWriteTs 为 null → 显示"无写入"而非延迟', async () => {
    sinks.mockResolvedValue({
      generatedAt: '2026-05-29T12:00:00Z',
      sinks: [mkSink({ lastWriteTs: null, lagSecs: 0 })],
    });
    const { container } = await renderPanel();
    await waitFor(() => expect(container.querySelector('.pb-sink')).toBeTruthy());
    expect(container.querySelector('.pb-sink .lag')).toHaveTextContent('无写入');
  });
});
