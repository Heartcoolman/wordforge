import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { UpdateBanner } from '@/components/ui/UpdateBanner';
import { setUpdateInfo } from '@/api/http';

describe('UpdateBanner', () => {
  beforeEach(() => setUpdateInfo(null));
  afterEach(() => setUpdateInfo(null));

  it('not rendered when no update info', () => {
    render(() => <UpdateBanner />);
    expect(screen.queryByText('刷新')).not.toBeInTheDocument();
  });

  it('renders message and refresh / close buttons when update info present', () => {
    setUpdateInfo({ version: '2.0.0', message: '有新版本可用' });
    render(() => <UpdateBanner />);
    expect(screen.getByText('有新版本可用')).toBeInTheDocument();
    expect(screen.getByText('刷新')).toBeInTheDocument();
    expect(screen.getByText('关闭')).toBeInTheDocument();
  });

  it('closing hides banner via setUpdateInfo(null)', () => {
    setUpdateInfo({ version: '2.0.0', message: '更新' });
    render(() => <UpdateBanner />);
    fireEvent.click(screen.getByText('关闭'));
    expect(screen.queryByText('更新')).not.toBeInTheDocument();
  });

  it('refresh button triggers window.location.reload', () => {
    setUpdateInfo({ version: '2.0.0', message: '更新' });
    const reloadMock = vi.fn();
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: { ...window.location, reload: reloadMock },
    });
    render(() => <UpdateBanner />);
    fireEvent.click(screen.getByText('刷新'));
    expect(reloadMock).toHaveBeenCalled();
  });
});
