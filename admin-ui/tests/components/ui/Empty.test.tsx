import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Empty } from '@/components/ui/Empty';

describe('Empty', () => {
  it('renders default icon', () => {
    const { container } = render(() => <Empty />);
    expect(container.querySelector('svg')).toBeInTheDocument();
  });

  it('shows title', () => {
    render(() => <Empty title="No data" />);
    expect(screen.getByText('No data')).toBeInTheDocument();
  });

  it('shows description', () => {
    render(() => <Empty description="Try again later" />);
    expect(screen.getByText('Try again later')).toBeInTheDocument();
  });

  it('shows action', () => {
    render(() => <Empty action={<button>Retry</button>} />);
    expect(screen.getByText('Retry')).toBeInTheDocument();
  });
});
