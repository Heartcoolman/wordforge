import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Input } from '@/components/ui/Input';

describe('Input', () => {
  it('renders with label', () => {
    render(() => <Input label="Email" />);
    expect(screen.getByText('Email')).toBeInTheDocument();
  });

  it('shows error message', () => {
    render(() => <Input error="Required" />);
    expect(screen.getByText('Required')).toBeInTheDocument();
  });

  it('shows hint when no error', () => {
    render(() => <Input hint="Enter your email" />);
    expect(screen.getByText('Enter your email')).toBeInTheDocument();
  });

  it('renders icon', () => {
    render(() => <Input icon={<span data-testid="ico" />} />);
    expect(screen.getByTestId('ico')).toBeInTheDocument();
  });

  it('renders rightIcon', () => {
    render(() => <Input rightIcon={<span data-testid="right" />} />);
    expect(screen.getByTestId('right')).toBeInTheDocument();
  });

  it('generates unique id', () => {
    render(() => (
      <>
        <Input label="A" />
        <Input label="B" />
      </>
    ));
    const inputs = screen.getAllByRole('textbox');
    const ids = inputs.map((el) => el.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});
