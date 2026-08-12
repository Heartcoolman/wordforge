import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, render } from '@solidjs/testing-library';
import type { AuditRow } from '@/types/probeTelemetry';

const audit = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
    audit: (...a: unknown[]) => audit(...a),
  },
}));

function mkRow(over: Partial<AuditRow> = {}): AuditRow {
  return {
    ts: '2026-05-29T11:59:00Z',
    action: 'mod',
    eventType: 'periodic',
    oldValue: '0.20',
    newValue: '0.35',
    adminId: 'admin-1',
    ...over,
  };
}

async function renderTrail() {
  const { AuditTrail } = await import('@/pages/probe/AuditTrail');
  return render(() => <AuditTrail />);
}

describe('AuditTrail', () => {
  beforeEach(() => vi.clearAllMocks());

  it('加载中显示 Spinner', async () => {
    audit.mockReturnValue(new Promise(() => {}));
    const { container } = await renderTrail();
    await waitFor(() => expect(screen.getByRole('status')).toBeInTheDocument());
    expect(container.querySelector('.pb-audit-row')).toBeNull();
  });

  it('无记录 → Empty 兜底', async () => {
    audit.mockResolvedValue({ rows: [] });
    await renderTrail();
    await waitFor(() => expect(screen.getByText('暂无改动记录')).toBeInTheDocument());
    expect(screen.getByText('probe_sampling_audit 表为空')).toBeInTheDocument();
  });

  it('按 limit=20 拉取并逐条渲染时间 / 操作人 / event_type', async () => {
    audit.mockResolvedValue({ rows: [mkRow(), mkRow({ eventType: 'error_js' })] });
    const { container } = await renderTrail();
    await waitFor(() => expect(container.querySelectorAll('.pb-audit-row')).toHaveLength(2));
    expect(audit).toHaveBeenCalledWith(20);
    // hms 走本地时区，只校验格式而非绝对值
    expect(container.querySelector('.pb-audit-row .ts')!.textContent).toMatch(/^\d{2}:\d{2}:\d{2}$/);
    expect(container.querySelector('.pb-audit-row .who')).toHaveTextContent('admin-1 修改 periodic');
    expect(screen.getByText('error_js')).toBeInTheDocument();
  });

  it('新旧值齐全 → 渲染 old → new；缺一则不渲染', async () => {
    audit.mockResolvedValue({
      rows: [mkRow(), mkRow({ oldValue: null, newValue: '1.00' })],
    });
    const { container } = await renderTrail();
    await waitFor(() => expect(container.querySelectorAll('.pb-audit-row')).toHaveLength(2));
    const rows = container.querySelectorAll('.pb-audit-row');
    expect(rows[0].querySelector('.who')).toHaveTextContent('0.20 → 0.35');
    expect(rows[1].querySelector('.who')!.textContent).not.toContain('→');
  });

  it('adminId 为 null → 操作人显示"系统"', async () => {
    audit.mockResolvedValue({ rows: [mkRow({ adminId: null })] });
    const { container } = await renderTrail();
    await waitFor(() => expect(container.querySelector('.pb-audit-row')).toBeTruthy());
    expect(container.querySelector('.pb-audit-row .who b')).toHaveTextContent('系统');
  });

  it('action → 中文动词 + badge class（未知 action 回落 mod/改动）', async () => {
    audit.mockResolvedValue({
      rows: [
        mkRow({ action: 'add' }),
        mkRow({ action: 'mod' }),
        mkRow({ action: 'pause' }),
        mkRow({ action: 'del' }),
        mkRow({ action: 'weird' }),
      ],
    });
    const { container } = await renderTrail();
    await waitFor(() => expect(container.querySelectorAll('.pb-audit-row')).toHaveLength(5));
    const badges = Array.from(container.querySelectorAll('.badge-mini'));
    expect(badges.map((b) => b.className)).toEqual([
      'badge-mini add',
      'badge-mini mod',
      'badge-mini pause',
      'badge-mini del',
      'badge-mini mod',
    ]);
    expect(badges.map((b) => b.textContent)).toEqual([
      '新增',
      '修改',
      '暂停/恢复',
      '删除',
      '改动',
    ]);
  });
});
