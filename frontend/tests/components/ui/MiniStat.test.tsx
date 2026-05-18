import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { MiniStat } from '@/components/ui/MiniStat';

describe('MiniStat', () => {
  it('renders label and value', () => {
    render(() => <MiniStat label="Active" value={42} />);
    expect(screen.getByText('Active')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
  });

  it('renders positive trend with up arrow', () => {
    render(() => <MiniStat label="X" value="10" trend={{ value: 5, label: 'WoW' }} />);
    expect(screen.getByText('↑')).toBeInTheDocument();
    expect(screen.getByText('5%')).toBeInTheDocument();
    expect(screen.getByText('WoW')).toBeInTheDocument();
  });

  it('renders negative trend with down arrow', () => {
    render(() => <MiniStat label="X" value="10" trend={{ value: -3, label: 'WoW' }} />);
    expect(screen.getByText('↓')).toBeInTheDocument();
    expect(screen.getByText('3%')).toBeInTheDocument();
  });

  it('renders zero trend with right arrow', () => {
    render(() => <MiniStat label="X" value="10" trend={{ value: 0, label: 'WoW' }} />);
    expect(screen.getByText('→')).toBeInTheDocument();
  });

  it('applies different tone colors', () => {
    const { unmount: u1 } = render(() => <MiniStat label="A" value="1" tone="success" />);
    expect(screen.getByText('1').className).toContain('text-success');
    u1();
    render(() => <MiniStat label="B" value="2" tone="error" />);
    expect(screen.getByText('2').className).toContain('text-error');
  });
});
