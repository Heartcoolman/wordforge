import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { SuggestionCard } from '@/pages/amas-advisor/SuggestionCard';
import type { AmasSuggestion, WhitelistRow } from '@/api/admin';

const base: AmasSuggestion = {
  id: 7, createdAt: '2026-05-29T10:00:00Z',
  basedOnVersionHash: 'abc1234567def890',
  patchJson: { 'memoryModel.baseDesiredRetention': 0.97 },
  rationale: '提升目标留存',
  evidenceJson: { fatigueDelta: -0.03, accuracyDelta: 0.012, retentionDelta: -0.005 },
  status: 'pending', decidedBy: null, decidedAt: null, decisionNote: null,
  costUsd: 0.01, tokensInput: 100, tokensOutput: 80, confidence: 0.9,
  baseValuesJson: { 'memoryModel.baseDesiredRetention': 0.92 },
};
const whitelist: WhitelistRow[] = [
  { path: 'memoryModel.baseDesiredRetention', minSafe: 0.8, maxSafe: 0.95 },
];

function noop() {}

describe('SuggestionCard 扩展', () => {
  it('渲染三联预估影响（疲劳/正确率/留存）', () => {
    render(() => (
      <SuggestionCard s={base} whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={noop} />
    ));
    expect(screen.getByText('疲劳率')).toBeInTheDocument();
    expect(screen.getByText('正确率')).toBeInTheDocument();
    expect(screen.getByText('留存')).toBeInTheDocument();
    // accuracyDelta 0.012 → +1.2%
    expect(screen.getByText(/\+1\.2/)).toBeInTheDocument();
  });

  it('evidence 缺字段时三联显 —', () => {
    render(() => (
      <SuggestionCard s={{ ...base, evidenceJson: {} }} whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={noop} />
    ));
    expect(screen.getAllByText('—').length).toBeGreaterThanOrEqual(3);
  });

  it('新值越白名单上界 → 标"越界"风险', () => {
    // 0.97 > maxSafe 0.95
    render(() => (
      <SuggestionCard s={base} whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={noop} />
    ));
    // 越界文案同时出现在卡级 badge 与行级风险列，用 getAllByText 容纳多处命中
    expect(screen.getAllByText(/越界/).length).toBeGreaterThanOrEqual(1);
  });

  it('白名单外参数 → 标"白名单外"', () => {
    render(() => (
      <SuggestionCard s={{ ...base, patchJson: { 'ensemble.weight': 0.5 } }}
        whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={noop} />
    ));
    // 白名单外文案同时出现在卡级 badge 与行级风险列
    expect(screen.getAllByText(/白名单外/).length).toBeGreaterThanOrEqual(1);
  });

  it('点击"进灰度"触发 onCanary', () => {
    const onCanary = vi.fn();
    render(() => (
      <SuggestionCard s={base} whitelist={whitelist} busy={false}
        onApprove={noop} onReject={noop} onCanary={onCanary} />
    ));
    fireEvent.click(screen.getByText(/进灰度/));
    expect(onCanary).toHaveBeenCalledTimes(1);
  });
});
