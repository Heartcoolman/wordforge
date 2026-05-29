import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { PatchCanaryCard } from '@/pages/amas-advisor/PatchCanaryCard';
import type { PatchCanary } from '@/api/admin';

const c: PatchCanary = {
  id: 3, suggestionId: 7, versionHash: 'deadbeef1234',
  percent: 20, cohortLo: 0, cohortHi: 20, status: 'active',
  baselineMetricsJson: '{"reward":0.5}',
  startedAt: '2026-05-29T10:00:00Z', updatedAt: '2026-05-29T10:05:00Z',
  liveReward: 0.55, liveAnomalyRate: 0.02, baselineReward: 0.5,
};

function noop() {}

describe('PatchCanaryCard', () => {
  it('渲染百分比 + live stat-pill', () => {
    render(() => (
      <PatchCanaryCard c={c} steps={[20, 60, 100]} busy={false}
        onScale={noop} onRollback={noop} onPromote={noop} />
    ));
    expect(screen.getByText(/20%/)).toBeInTheDocument();
    expect(screen.getByText(/reward 0\.55/)).toBeInTheDocument();
    // liveReward 0.55 > baseline 0.5 → 升幅 +0.05
    expect(screen.getByText(/\+0\.05/)).toBeInTheDocument();
  });

  it('扩量按钮传下一档百分比', () => {
    const onScale = vi.fn();
    render(() => (
      <PatchCanaryCard c={c} steps={[20, 60, 100]} busy={false}
        onScale={onScale} onRollback={noop} onPromote={noop} />
    ));
    fireEvent.click(screen.getByText(/扩量到 60%/));
    expect(onScale).toHaveBeenCalledWith(60);
  });

  it('100% 时显示 promote、隐藏扩量', () => {
    const onPromote = vi.fn();
    render(() => (
      <PatchCanaryCard c={{ ...c, percent: 100, cohortHi: 100 }} steps={[20, 60, 100]} busy={false}
        onScale={noop} onRollback={noop} onPromote={onPromote} />
    ));
    expect(screen.queryByText(/扩量到/)).toBeNull();
    fireEvent.click(screen.getByText(/提升为 stable/));
    expect(onPromote).toHaveBeenCalledTimes(1);
  });

  it('回滚按钮触发 onRollback', () => {
    const onRollback = vi.fn();
    render(() => (
      <PatchCanaryCard c={c} steps={[20, 60, 100]} busy={false}
        onScale={noop} onRollback={onRollback} onPromote={noop} />
    ));
    fireEvent.click(screen.getByText('回滚'));
    expect(onRollback).toHaveBeenCalledTimes(1);
  });
});
