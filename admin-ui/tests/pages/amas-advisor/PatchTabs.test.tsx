import { describe, it, expect, vi } from 'vitest';
import { screen, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { PatchTabs, type PatchTabId } from '@/pages/amas-advisor/PatchTabs';

const counts = { pending: 2, canary: 3, effective: 42, rejected: 6 };

describe('PatchTabs', () => {
  it('四 tab 含计数角标', () => {
    renderWithProviders(() => (
      <PatchTabs active="pending" counts={counts} onChange={() => {}} nowMs={0} />
    ));
    expect(screen.getByRole('tab', { name: /待审.*2/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /灰度中.*3/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /已生效.*42/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /已拒绝.*6/ })).toBeInTheDocument();
  });

  it('点击切换回调', () => {
    const onChange = vi.fn();
    renderWithProviders(() => (
      <PatchTabs active="pending" counts={counts} onChange={onChange} nowMs={0} />
    ));
    fireEvent.click(screen.getByRole('tab', { name: /灰度中/ }));
    expect(onChange).toHaveBeenCalledWith('canary' satisfies PatchTabId);
  });

  it('倒计时按 20min 周期显示剩余', () => {
    // nowMs = 第 12 分钟 → 距下次巡查 8 分 0 秒
    const twelveMin = 12 * 60 * 1000;
    renderWithProviders(() => (
      <PatchTabs active="pending" counts={counts} onChange={() => {}} nowMs={twelveMin} />
    ));
    expect(screen.getByText(/下次巡查/)).toHaveTextContent(/8 分/);
  });
});
