import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Pagination } from '@/components/ui/Pagination';

describe('Pagination — page-window branches', () => {
  it('shows ellipsis when total pages exceed 7', () => {
    render(() => <Pagination page={5} total={200} pageSize={10} onChange={() => {}} />);
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

describe('Pagination — handlers & class prop', () => {
  it('calls onChange when 上一页 clicked', () => {
    const onChange = vi.fn();
    render(() => <Pagination page={3} total={30} pageSize={10} onChange={onChange} />);
    fireEvent.click(screen.getByLabelText('上一页'));
    expect(onChange).toHaveBeenCalledWith(2);
  });

  it('calls onChange when 下一页 clicked', () => {
    const onChange = vi.fn();
    render(() => <Pagination page={1} total={30} pageSize={10} onChange={onChange} />);
    fireEvent.click(screen.getByLabelText('下一页'));
    expect(onChange).toHaveBeenCalledWith(2);
  });

  it('passes custom class prop', () => {
    const { container } = render(() => (
      <Pagination page={1} total={30} pageSize={10} onChange={() => {}} class="custom-cls" />
    ));
    expect(container.querySelector('nav')?.className).toContain('custom-cls');
  });
});
