import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent, render } from '@solidjs/testing-library';
import type { ProbeRow, AuditRow } from '@/types/probeTelemetry';

const schema = vi.fn();
const audit = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
    schema: (...a: unknown[]) => schema(...a),
    audit: (...a: unknown[]) => audit(...a),
  },
}));

function mkProbe(over: Partial<ProbeRow> = {}): ProbeRow {
  return {
    key: 'click',
    label: '点击行为',
    desc: '用户在界面上的点击、滑动等操作',
    count24h: 5200,
    eps: 0.06,
    // 相对时间断言需可预期：固定为 1 分钟前
    lastTs: new Date(Date.now() - 60_000).toISOString(),
    sampleRate: 0.35,
    locked: false,
    enabled: true,
    samplingEventType: 'periodic',
    ...over,
  };
}

function mkAudit(over: Partial<AuditRow> = {}): AuditRow {
  return {
    ts: new Date(Date.now() - 3_600_000).toISOString(),
    action: 'mod',
    eventType: 'periodic',
    oldValue: '0.20',
    newValue: '0.35',
    adminId: 'admin-1',
    ...over,
  };
}

async function renderDrawer(probe: ProbeRow, open = true, onClose = vi.fn()) {
  const { ProbeDetailDrawer } = await import('@/pages/probe/ProbeDetailDrawer');
  const r = render(() => <ProbeDetailDrawer probe={probe} open={open} onClose={onClose} />);
  return { ...r, onClose };
}

describe('ProbeDetailDrawer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    schema.mockResolvedValue({ eventType: 'periodic', sampledAt: null, payload: null });
    audit.mockResolvedValue({ rows: [] });
  });

  it('open=false → 不渲染抽屉，也不拉取 schema / audit', async () => {
    await renderDrawer(mkProbe(), false);
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(schema).not.toHaveBeenCalled();
    expect(audit).not.toHaveBeenCalled();
  });

  it('open=true → 渲染标题与全部属性行', async () => {
    await renderDrawer(mkProbe());
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveAttribute('aria-label', '探针详情 · 点击行为');
    expect(screen.getByText('探针详情 · 点击行为')).toBeInTheDocument();
    expect(screen.getByText('click')).toBeInTheDocument();
    expect(screen.getByText('用户在界面上的点击、滑动等操作')).toBeInTheDocument();
    expect(screen.getByText((5200).toLocaleString())).toBeInTheDocument();
    expect(screen.getByText('0.06')).toBeInTheDocument();
    expect(screen.getByText('1 分钟前')).toBeInTheDocument();
    expect(screen.getByText('35%')).toBeInTheDocument();
    expect(screen.getByText('启用')).toBeInTheDocument();
    // 有 samplingEventType 才渲染该行
    expect(screen.getByText('采样 event_type')).toBeInTheDocument();
  });

  it('点击关闭按钮触发 onClose', async () => {
    const { onClose } = await renderDrawer(mkProbe());
    await screen.findByRole('dialog');
    fireEvent.click(screen.getByLabelText('关闭'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('locked / 停用 状态文案分支', async () => {
    const { unmount } = await renderDrawer(mkProbe({ locked: true }));
    await waitFor(() => expect(screen.getByText('锁定 · 核心数据强制 100%')).toBeInTheDocument());
    unmount();

    await renderDrawer(mkProbe({ locked: false, enabled: false }));
    await waitFor(() => expect(screen.getByText('已停用')).toBeInTheDocument());
  });

  it('无 samplingEventType → 诚实说明无独立 schema，且不请求 schema / audit', async () => {
    const probe = mkProbe();
    delete probe.samplingEventType;
    await renderDrawer(probe);
    await screen.findByRole('dialog');
    expect(
      screen.getByText(/不经 telemetry 采样管道，因此无独立 payload schema/),
    ).toBeInTheDocument();
    expect(screen.queryByText('采样 event_type')).toBeNull();
    expect(schema).not.toHaveBeenCalled();
    expect(audit).not.toHaveBeenCalled();
  });

  it('有 samplingEventType → 拉 schema 与 audit，payload 为空走 Empty', async () => {
    await renderDrawer(mkProbe());
    await screen.findByRole('dialog');
    await waitFor(() => expect(schema).toHaveBeenCalledWith('periodic'));
    expect(audit).toHaveBeenCalledWith(50);
    await waitFor(() => expect(screen.getByText('暂无样本')).toBeInTheDocument());
    expect(screen.getByText('periodic 暂无 telemetry_events 落库样本')).toBeInTheDocument();
    expect(screen.getByText('暂无变更')).toBeInTheDocument();
    expect(screen.getByText('该 event_type 采样配置未改动过')).toBeInTheDocument();
  });

  it('有 payload → 渲染 pretty JSON', async () => {
    schema.mockResolvedValue({
      eventType: 'periodic',
      sampledAt: '2026-05-29T12:00:00Z',
      payload: { clickCount: 42 },
    });
    const { container } = await renderDrawer(mkProbe());
    await waitFor(() => expect(document.querySelector('.pb-schema')).toBeTruthy());
    // Drawer 走 Portal，节点不在 render container 内
    expect(container.querySelector('.pb-schema')).toBeNull();
    expect(document.querySelector('.pb-schema')!.textContent).toContain('"clickCount": 42');
  });

  it('审计只保留同 event_type 的行并渲染新旧值', async () => {
    audit.mockResolvedValue({
      rows: [
        mkAudit(),
        mkAudit({ eventType: 'error_js', action: 'add' }),
        mkAudit({ action: 'pause', oldValue: null, newValue: null }),
      ],
    });
    await renderDrawer(mkProbe());
    await waitFor(() => expect(document.querySelectorAll('ul li')).toHaveLength(2));
    const items = Array.from(document.querySelectorAll('ul li')).map((li) => li.textContent);
    expect(items[0]).toContain('mod');
    expect(items[0]).toContain('0.20');
    expect(items[0]).toContain('0.35');
    expect(items[0]).toContain('1 小时前');
    // oldValue / newValue 为 null 时占位破折号
    expect(items[1]).toContain('—');
    expect(screen.queryByText('暂无变更')).toBeNull();
  });

  it('schema 请求未落地时展示 Spinner', async () => {
    schema.mockReturnValue(new Promise(() => {}));
    await renderDrawer(mkProbe());
    await screen.findByRole('dialog');
    await waitFor(() => expect(screen.getAllByRole('status').length).toBeGreaterThan(0));
  });
});
