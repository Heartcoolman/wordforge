import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { HeroCard } from '@/components/ui/HeroCard';

describe('HeroCard', () => {
  it('renders title and desc', () => {
    render(() => <HeroCard title="全局概览" desc="今日学习产出汇总" />);
    expect(screen.getByText('全局概览')).toBeInTheDocument();
    expect(screen.getByText('今日学习产出汇总')).toBeInTheDocument();
  });

  it('renders eyebrow chip with default accent variant', () => {
    render(() => <HeroCard eyebrow="提案版" />);
    expect(screen.getByText('提案版')).toBeInTheDocument();
  });

  it('renders meta-grid cells with number values (count-up) and string values', () => {
    render(() => (
      <HeroCard
        title="t"
        meta={[
          { value: 100, label: 'A' },
          { value: 'oklch', label: 'B' },
        ]}
      />
    ));
    expect(screen.getByText('A')).toBeInTheDocument();
    expect(screen.getByText('B')).toBeInTheDocument();
    // string value 直显
    expect(screen.getByText('oklch')).toBeInTheDocument();
  });

  it('renders cta region when provided', () => {
    render(() => <HeroCard title="t" cta={<button>refresh</button>} />);
    expect(screen.getByRole('button', { name: 'refresh' })).toBeInTheDocument();
  });

  it('supports <em> accent gradient highlight in title', () => {
    render(() => <HeroCard title="设计 <em>系统</em>" />);
    // title 通过 innerHTML 注入，<em> 应该作为元素存在
    expect(document.querySelector('em')).toBeTruthy();
  });
});
