import { describe, it, expect } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { CostRow } from '@/pages/amas-advisor/CostRow';
import type { AdvisorCostStats } from '@/api/admin';

const stats: AdvisorCostStats = {
  monthYuan: 4.21, monthCapYuan: 10, quotaPct: 42.1, forecastYuan: 6.84,
  avg7dCostYuan: 0.14, monthCalls: 31, acceptedCount: 47, rejectedCount: 6, acceptanceRate: 0.887,
  usdToCny: 7.3,
};

describe('CostRow', () => {
  it('渲染本月成本/配额/接受率', () => {
    renderWithProviders(() => <CostRow stats={stats} />);
    expect(screen.getByText('¥4.21')).toBeInTheDocument();
    expect(screen.getByText(/¥10/)).toBeInTheDocument();
    expect(screen.getByText(/42\.1%/)).toBeInTheDocument();
    expect(screen.getByText('47/53')).toBeInTheDocument(); // accepted/(accepted+rejected)
  });

  it('配额条 width 反映 quotaPct', () => {
    const { container } = renderWithProviders(() => <CostRow stats={stats} />);
    const bar = container.querySelector('[data-testid="quota-bar-fill"]') as HTMLElement;
    expect(bar.style.width).toBe('42.1%');
  });
});
