import { describe, it, expect } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { CostChart } from '@/pages/amas-advisor/CostChart';
import type { AdvisorCostDaily } from '@/api/admin';

const data: AdvisorCostDaily[] = Array.from({ length: 30 }, (_, i) => ({
  date: `2026-05-${String(i + 1).padStart(2, '0')}`,
  costYuan: (i % 5) * 0.05,
}));

describe('CostChart', () => {
  it('渲染 30 根柱 + 参考线 + footer', () => {
    const { container } = renderWithProviders(() => (
      <CostChart data={data} avg7dYuan={0.14} capYuan={10} refLineYuan={0.3} />
    ));
    expect(container.querySelectorAll('rect[data-bar]').length).toBe(30);
    expect(screen.getByText(/7 天平均/)).toHaveTextContent('¥0.14');
    expect(screen.getByText(/月度上限/)).toHaveTextContent('¥10');
  });

  it('空数据显示占位', () => {
    renderWithProviders(() => <CostChart data={[]} avg7dYuan={0} capYuan={10} refLineYuan={0.3} />);
    expect(screen.getByText(/暂无成本数据/)).toBeInTheDocument();
  });
});
