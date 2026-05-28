import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Panel } from '@/components/ui/Panel';

describe('Panel', () => {
  it('renders title and children', () => {
    render(() => (
      <Panel title="标题">
        <p>主体内容</p>
      </Panel>
    ));
    expect(screen.getByText('标题')).toBeInTheDocument();
    expect(screen.getByText('主体内容')).toBeInTheDocument();
  });

  it('renders actions slot', () => {
    render(() => (
      <Panel title="标题" actions={<button>动作</button>}>
        <p>主体</p>
      </Panel>
    ));
    expect(screen.getByText('动作')).toBeInTheDocument();
  });

  it('renders aside slot', () => {
    render(() => (
      <Panel title="标题" aside={<aside>侧栏</aside>}>
        <p>主体</p>
      </Panel>
    ));
    expect(screen.getByText('侧栏')).toBeInTheDocument();
  });
});
