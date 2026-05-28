import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Pagination } from '@/components/ui/Pagination';

describe('Pagination', () => {
  it('not rendered when totalPages <= 1', () => {
    const { container } = render(() =>
      <Pagination page={1} total={5} pageSize={10} onChange={() => {}} />);
    expect(container.querySelector('nav')).not.toBeInTheDocument();
  });

  it('renders page buttons', () => {
    render(() => <Pagination page={1} total={30} pageSize={10} onChange={() => {}} />);
    expect(screen.getByLabelText('第 1 页')).toBeInTheDocument();
    expect(screen.getByLabelText('第 3 页')).toBeInTheDocument();
  });

  it('calls onChange on page click', async () => {
    const onChange = vi.fn();
    render(() => <Pagination page={1} total={30} pageSize={10} onChange={onChange} />);
    await fireEvent.click(screen.getByLabelText('第 2 页'));
    expect(onChange).toHaveBeenCalledWith(2);
  });

  it('disables prev on first page', () => {
    render(() => <Pagination page={1} total={30} pageSize={10} onChange={() => {}} />);
    expect(screen.getByLabelText('上一页')).toBeDisabled();
  });

  it('disables next on last page', () => {
    render(() => <Pagination page={3} total={30} pageSize={10} onChange={() => {}} />);
    expect(screen.getByLabelText('下一页')).toBeDisabled();
  });
});
