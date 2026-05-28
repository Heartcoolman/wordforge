import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { SystemLockedModal } from '@/components/SystemLockedModal';

// 重构后 SystemLockedModal 是 Modal 组件的薄包装（hideClose），
// focus/escape/backdrop 等行为由 Modal 自己测试覆盖，这里只断言文案。
describe('SystemLockedModal', () => {
  it('renders title and body copy', () => {
    render(() => <SystemLockedModal />);
    expect(screen.getByText('数据损坏')).toBeInTheDocument();
    expect(screen.getByText(/客户端数据已损坏/)).toBeInTheDocument();
  });
});
