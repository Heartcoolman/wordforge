import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Table } from '@/components/ui/Table';

const columns = [
  { key: 'name', title: 'Name' },
  { key: 'age', title: 'Age' },
];

describe('Table', () => {
  it('renders column headers', () => {
    render(() => <Table columns={columns} data={[]} />);
    expect(screen.getByText('Name')).toBeInTheDocument();
    expect(screen.getByText('Age')).toBeInTheDocument();
  });

  it('renders data rows', () => {
    const data = [{ name: 'Alice', age: 30 }, { name: 'Bob', age: 25 }];
    render(() => <Table columns={columns} data={data} />);
    expect(screen.getByText('Alice')).toBeInTheDocument();
    expect(screen.getByText('25')).toBeInTheDocument();
  });

  it('shows empty text when no data', () => {
    render(() => <Table columns={columns} data={[]} emptyText="Nothing here" />);
    expect(screen.getByText('Nothing here')).toBeInTheDocument();
  });

  it('shows loading skeleton', () => {
    const { container } = render(() => <Table columns={columns} data={[]} loading />);
    // 升级：表格 loading 行从 animate-pulse 改为 animate-shimmer 复用 Skeleton 样式
    expect(container.querySelectorAll('.animate-shimmer').length).toBeGreaterThan(0);
  });

  it('calls onRowClick on click', async () => {
    const onClick = vi.fn();
    const data = [{ name: 'Alice', age: 30 }];
    render(() => <Table columns={columns} data={data} onRowClick={onClick} />);
    await fireEvent.click(screen.getByText('Alice'));
    expect(onClick).toHaveBeenCalledWith(data[0]);
  });

  it('headers carry scope="col"', () => {
    render(() => <Table columns={columns} data={[]} />);
    expect(screen.getByText('Name').getAttribute('scope')).toBe('col');
  });

  it('renders sr-only caption when provided', () => {
    const { container } = render(() => <Table columns={columns} data={[]} caption="用户列表" />);
    const cap = container.querySelector('caption');
    expect(cap?.textContent).toBe('用户列表');
    expect(cap?.className).toContain('sr-only');
  });

  it('onRowClick rows gain role=button and Enter triggers click', async () => {
    const onClick = vi.fn();
    const data = [{ name: 'Alice', age: 30 }];
    render(() => <Table columns={columns} data={data} onRowClick={onClick} />);
    const row = screen.getByText('Alice').closest('tr')!;
    expect(row.getAttribute('role')).toBe('button');
    expect(row.getAttribute('tabindex')).toBe('0');
    await fireEvent.keyDown(row, { key: 'Enter' });
    expect(onClick).toHaveBeenCalledWith(data[0]);
  });

  it('non-clickable rows have no role/tabindex', () => {
    const data = [{ name: 'Alice', age: 30 }];
    render(() => <Table columns={columns} data={data} />);
    const row = screen.getByText('Alice').closest('tr')!;
    expect(row.getAttribute('role')).toBeNull();
    expect(row.getAttribute('tabindex')).toBeNull();
  });
});
