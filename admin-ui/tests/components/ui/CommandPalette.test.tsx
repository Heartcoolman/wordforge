import { describe, it, expect, vi } from 'vitest';
import { screen, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { CommandPalette } from '@/components/ui/CommandPalette';

/** Router 包装：CommandPalette 用 useNavigate，需要 RouterContext */
function withRouter(ui: () => any) {
  return renderWithProviders(ui);
}

describe('CommandPalette', () => {
  it('opens on ⌘K (metaKey)', async () => {
    withRouter(() => <CommandPalette />);
    // 模拟全局 ⌘K
    fireEvent.keyDown(document, { key: 'k', metaKey: true });
    expect(await screen.findByRole('dialog', { name: '命令面板' })).toBeInTheDocument();
  });

  it('opens on Ctrl+K (Linux/Windows)', async () => {
    withRouter(() => <CommandPalette />);
    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
  });

  it('filters items by query (fuzzy contains on title + desc + keywords)', async () => {
    withRouter(() => <CommandPalette open={true} />);
    const input = await screen.findByPlaceholderText('跳转到任意页面或操作…');
    fireEvent.input(input, { target: { value: 'amas' } });
    // 至少匹配到 amas-config / amas-metrics / amas-advisor 3 项
    expect(screen.queryByText('AMAS 调参')).toBeInTheDocument();
    expect(screen.queryByText('AMAS 指标')).toBeInTheDocument();
    expect(screen.queryByText('LLM 调参顾问')).toBeInTheDocument();
  });

  it('shows "无匹配项" when query has zero matches', async () => {
    withRouter(() => <CommandPalette open={true} />);
    const input = await screen.findByPlaceholderText('跳转到任意页面或操作…');
    fireEvent.input(input, { target: { value: 'zzzzz_no_match_xxx' } });
    expect(screen.getByText('无匹配项')).toBeInTheDocument();
  });

  it('closes on Escape', async () => {
    const [, setExternal] = [() => true, vi.fn()];
    withRouter(() => <CommandPalette open={true} onOpenChange={setExternal} />);
    const input = await screen.findByPlaceholderText('跳转到任意页面或操作…');
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(setExternal).toHaveBeenCalledWith(false);
  });

  it('ArrowDown moves selection to next item', async () => {
    withRouter(() => <CommandPalette open={true} />);
    const input = await screen.findByPlaceholderText('跳转到任意页面或操作…');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    // 第 2 个 item（index 1）现在 aria-selected=true
    const items = screen.getAllByRole('option');
    expect(items[1].getAttribute('aria-selected')).toBe('true');
  });
});
