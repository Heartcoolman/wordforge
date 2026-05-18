import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import MaintenancePage from '@/pages/MaintenancePage';

describe('MaintenancePage', () => {
  it('renders maintenance title and copy', () => {
    render(() => <MaintenancePage />);
    expect(screen.getByText('服务器维护中')).toBeInTheDocument();
    expect(screen.getByText(/系统正在进行维护升级/)).toBeInTheDocument();
  });
});
