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
    expect(container.querySelectorAll('.animate-pulse').length).toBeGreaterThan(0);
  });

  it('calls onRowClick', async () => {
    const onClick = vi.fn();
    const data = [{ name: 'Alice', age: 30 }];
    render(() => <Table columns={columns} data={data} onRowClick={onClick} />);
    await fireEvent.click(screen.getByText('Alice'));
    expect(onClick).toHaveBeenCalledWith(data[0]);
  });
});
