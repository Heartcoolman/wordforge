import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Empty } from '@/components/ui/Empty';

describe('Empty extra — icon override branch', () => {
  it('renders provided icon instead of default svg', () => {
    render(() => <Empty icon={<i data-testid="custom-icon">X</i>} title="t" />);
    expect(screen.getByTestId('custom-icon')).toBeInTheDocument();
  });

  it('applies custom class', () => {
    const { container } = render(() => <Empty class="my-empty" />);
    expect(container.firstChild).toHaveClass('my-empty');
  });
});
