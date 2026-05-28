import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { TextArea } from '@/components/ui/Input';

describe('TextArea', () => {
  it('renders with label', () => {
    render(() => <TextArea label="Notes" />);
    expect(screen.getByText('Notes')).toBeInTheDocument();
  });

  it('shows error message', () => {
    render(() => <TextArea error="Required" />);
    expect(screen.getByText('Required')).toBeInTheDocument();
  });

  it('generates unique ids when none passed', () => {
    render(() => (
      <>
        <TextArea label="A" />
        <TextArea label="B" />
      </>
    ));
    const tas = screen.getAllByRole('textbox');
    const ids = tas.map((el) => el.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('uses caller-supplied id', () => {
    render(() => <TextArea id="custom-id" label="X" />);
    expect(document.getElementById('custom-id')).toBeInTheDocument();
  });
});
