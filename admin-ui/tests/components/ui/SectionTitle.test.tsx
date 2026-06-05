import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { SectionTitle } from '@/components/ui/SectionTitle';

describe('SectionTitle', () => {
  it('renders title as h2 by default', () => {
    render(() => <SectionTitle title="设计系统总览" />);
    const h2 = screen.getByText('设计系统总览');
    expect(h2.tagName).toBe('H2');
  });

  it('renders as h3 when as="h3"', () => {
    render(() => <SectionTitle as="h3" title="子分区" />);
    const h3 = screen.getByText('子分区');
    expect(h3.tagName).toBe('H3');
  });

  it('renders sub text in gray when no actions', () => {
    render(() => <SectionTitle title="t" sub="副说明" />);
    expect(screen.getByText('副说明')).toBeInTheDocument();
  });

  it('prefers actions over sub when both provided', () => {
    render(() => <SectionTitle title="t" sub="副说明" actions={<button>act</button>} />);
    expect(screen.getByRole('button', { name: 'act' })).toBeInTheDocument();
    // 副说明在 actions 优先时不渲染
    expect(screen.queryByText('副说明')).not.toBeInTheDocument();
  });
});
