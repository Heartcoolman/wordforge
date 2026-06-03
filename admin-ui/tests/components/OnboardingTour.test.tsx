import { describe, it, expect, beforeEach } from 'vitest';
import { screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { OnboardingTour } from '@/components/onboarding/OnboardingTour';
import { useOnboarding, shouldAutoShow, markSeen, waveOf } from '@/components/onboarding/useOnboarding';

const STORAGE_KEY = 'wf_admin_onboarding_wave';
const WAVE = 'v1.1.3';        // 当前波次（v1.1.3 运维闭环新功能导览）
const OLD_WAVE = 'v1.1-1.2';  // 1.1.0–1.1.2 + 1.2.x 旧 admin 重构波
const TOTAL = 6;              // STEPS 步数（welcome + 4 功能屏 + outro）

function renderTour() {
  const ctl = useOnboarding();
  // 模拟 AdminLayout 注入当前版本，设好 currentWave 供 markSeen 写回
  shouldAutoShow('1.1.3-beta.1');
  renderWithProviders(() => <OnboardingTour />);
  ctl.show();
  return ctl;
}

describe('useOnboarding 波次逻辑', () => {
  beforeEach(() => localStorage.clear());

  it('waveOf：v1.1.3 起独立波次；1.1.0–1.1.2+1.2.x 旧波；1.3+ 各自波', () => {
    expect(waveOf('1.1.3-beta.1')).toBe(WAVE);
    expect(waveOf('1.1.3')).toBe(WAVE);
    expect(waveOf('1.1.2-beta.2')).toBe(OLD_WAVE);
    expect(waveOf('1.2.0')).toBe(OLD_WAVE);
    expect(waveOf('1.2.9')).toBe(OLD_WAVE);
    expect(waveOf('1.3.0')).toBe('1.3');
    expect(waveOf('2.0.0')).toBe('2.0');
    expect(waveOf(null)).toBeNull();
    expect(waveOf(undefined)).toBeNull();
  });

  it('未看过本波时 shouldAutoShow 返回 true；拿不到版本返回 false', () => {
    expect(shouldAutoShow('1.1.3-beta.1')).toBe(true);
    expect(shouldAutoShow(null)).toBe(false);
  });

  it('看过旧波(v1.1-1.2)升到 1.1.3 仍重弹；同 v1.1.3 波不再弹；跨大版本仍弹', () => {
    shouldAutoShow('1.1.2'); // 设 currentWave=旧波
    markSeen();
    expect(localStorage.getItem(STORAGE_KEY)).toBe(OLD_WAVE);
    // 升到 1.1.3 → 新波，重弹一次
    expect(shouldAutoShow('1.1.3-beta.1')).toBe(true);
    markSeen();
    expect(localStorage.getItem(STORAGE_KEY)).toBe(WAVE);
    // 同 v1.1.3 波内重复升级 → 不弹
    expect(shouldAutoShow('1.1.3')).toBe(false);
    // 跨入下一大版本 → 重新弹
    expect(shouldAutoShow('1.3.0')).toBe(true);
  });
});

describe('OnboardingTour', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboarding().close();
  });

  it('渲染首屏标题（欢迎屏）', async () => {
    renderTour();
    expect(await screen.findByRole('dialog', { name: '新功能导览' })).toBeInTheDocument();
    expect(screen.getByText('WordForge Admin v1.1.3')).toBeInTheDocument();
    expect(screen.getByText(`1 / ${TOTAL}`)).toBeInTheDocument();
  });

  it('点下一步切到第二屏（告警收件箱）', async () => {
    renderTour();
    await screen.findByText('WordForge Admin v1.1.3');
    fireEvent.click(screen.getByRole('button', { name: '下一步' }));
    expect(await screen.findByText('告警收件箱')).toBeInTheDocument();
    expect(screen.getByText(`2 / ${TOTAL}`)).toBeInTheDocument();
  });

  it('走到完成后 markSeen 写入波次且同波 shouldAutoShow=false', async () => {
    const ctl = renderTour();
    await screen.findByText('WordForge Admin v1.1.3');
    // 连点下一步至最后一屏，再点完成
    for (let i = 0; i < TOTAL - 1; i++) {
      fireEvent.click(screen.getByRole('button', { name: '下一步' }));
    }
    await screen.findByText('随时可以再看一遍');
    // 末屏同时有主按钮与底部「完成」，点任意一个均 markSeen + 关闭
    fireEvent.click(screen.getAllByRole('button', { name: '完成' })[0]);
    await waitFor(() => expect(ctl.open()).toBe(false));
    expect(localStorage.getItem(STORAGE_KEY)).toBe(WAVE);
    expect(shouldAutoShow('1.1.3')).toBe(false);
  });

  it('跳过后 markSeen 写入且关闭', async () => {
    const ctl = renderTour();
    await screen.findByText('WordForge Admin v1.1.3');
    fireEvent.click(screen.getByRole('button', { name: '跳过导览' }));
    await waitFor(() => expect(ctl.open()).toBe(false));
    expect(shouldAutoShow('1.1.3')).toBe(false);
  });

  it('重看入口能再次打开', async () => {
    const ctl = renderTour();
    await screen.findByText('WordForge Admin v1.1.3');
    fireEvent.click(screen.getByRole('button', { name: '跳过导览' }));
    await waitFor(() => expect(ctl.open()).toBe(false));
    // 重看：show() 不依赖 localStorage
    ctl.show();
    expect(await screen.findByRole('dialog', { name: '新功能导览' })).toBeInTheDocument();
  });
});
