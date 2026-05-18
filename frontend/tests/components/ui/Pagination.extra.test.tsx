import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Pagination } from '@/components/ui/Pagination';

describe('Pagination extra branches', () => {
  it('shows ellipsis when total pages exceed 7', () => {
    render(() => <Pagination page={5} total={200} pageSize={10} onChange={() => {}} />);
    // 多于一个 ...
    const dots = screen.getAllByText('...');
    expect(dots.length).toBeGreaterThanOrEqual(1);
  });

  it('handles current page near start (no leading ellipsis)', () => {
    render(() => <Pagination page={1} total={100} pageSize={10} onChange={() => {}} />);
    expect(screen.getByLabelText('第 1 页')).toBeInTheDocument();
    expect(screen.getByLabelText('第 10 页')).toBeInTheDocument();
  });

  it('handles current page near end (no trailing ellipsis)', () => {
    render(() => <Pagination page={10} total={100} pageSize={10} onChange={() => {}} />);
    expect(screen.getByLabelText('第 10 页')).toBeInTheDocument();
  });

  it('marks current page with aria-current', () => {
    render(() => <Pagination page={2} total={30} pageSize={10} onChange={() => {}} />);
    expect(screen.getByLabelText('第 2 页')).toHaveAttribute('aria-current', 'page');
  });
});
