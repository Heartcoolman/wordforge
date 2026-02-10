import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Select } from '@/components/ui/Select';

const options = [
  { value: 'a', label: 'Alpha' },
  { value: 'b', label: 'Beta' },
];

describe('Select', () => {
  it('renders options', () => {
    render(() => <Select options={options} />);
    expect(screen.getByText('Alpha')).toBeInTheDocument();
    expect(screen.getByText('Beta')).toBeInTheDocument();
  });

  it('shows label', () => {
    render(() => <Select options={options} label="Pick one" />);
    expect(screen.getByText('Pick one')).toBeInTheDocument();
  });

  it('shows placeholder', () => {
    render(() => <Select options={options} placeholder="Choose..." />);
    expect(screen.getByText('Choose...')).toBeInTheDocument();
  });

  it('shows error', () => {
    render(() => <Select options={options} error="Required" />);
    expect(screen.getByText('Required')).toBeInTheDocument();
  });

  it('generates unique id', () => {
    render(() => (
      <>
        <Select options={options} label="X" />
        <Select options={options} label="Y" />
      </>
    ));
    const selects = screen.getAllByRole('combobox');
    const ids = selects.map((el) => el.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});
