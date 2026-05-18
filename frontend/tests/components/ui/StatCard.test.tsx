import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { StatCard } from '@/components/ui/StatCard';

describe('StatCard', () => {
  it('renders title and value', () => {
    render(() => <StatCard title="Users" value={123} icon="M0 0h1v1H0z" color="accent" />);
    expect(screen.getByText('Users')).toBeInTheDocument();
    expect(screen.getByText('123')).toBeInTheDocument();
  });

  it('renders positive trend (up)', () => {
    render(() => <StatCard title="X" value="1" icon="" color="success" trend={{ value: 5, label: 'WoW' }} />);
    expect(screen.getByText(/↑/)).toBeInTheDocument();
    expect(screen.getByText(/5%/)).toBeInTheDocument();
  });

  it('renders negative trend (down)', () => {
    render(() => <StatCard title="X" value="1" icon="" color="warning" trend={{ value: -2, label: 'WoW' }} />);
    expect(screen.getByText(/↓/)).toBeInTheDocument();
  });

  it('renders zero trend (right)', () => {
    render(() => <StatCard title="X" value="1" icon="" color="info" trend={{ value: 0, label: 'WoW' }} />);
    expect(screen.getByText(/→/)).toBeInTheDocument();
  });

  it('applies error color class', () => {
    render(() => <StatCard title="Failures" value="5" icon="" color="error" />);
    expect(screen.getByText('5').className).toContain('text-error');
  });
});
