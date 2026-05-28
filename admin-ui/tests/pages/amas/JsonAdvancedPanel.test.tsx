import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { JsonAdvancedPanel } from '@/pages/amas/JsonAdvancedPanel';

describe('JsonAdvancedPanel', () => {
  it('renders textarea with serialized config', () => {
    render(() => <JsonAdvancedPanel config={{ a: 1 }} onChange={() => {}} />);
    const ta = document.querySelector('textarea') as HTMLTextAreaElement;
    expect(ta).toBeInstanceOf(HTMLTextAreaElement);
    expect(ta.value).toContain('"a": 1');
  });

  it('applies valid JSON via 应用到表单 button', () => {
    const onChange = vi.fn();
    render(() => <JsonAdvancedPanel config={{ a: 1 }} onChange={onChange} />);
    const ta = document.querySelector('textarea') as HTMLTextAreaElement;
    // 非受控 + draft signal：必须通过 input 事件让 draft 更新，按钮才会变为可点
    fireEvent.input(ta, { target: { value: '{"b": 2}' } });
    fireEvent.click(screen.getByText('应用到表单'));
    expect(onChange).toHaveBeenCalledWith({ b: 2 });
  });

  it('rejects non-object JSON (array)', () => {
    const onChange = vi.fn();
    render(() => <JsonAdvancedPanel config={{ a: 1 }} onChange={onChange} />);
    const ta = document.querySelector('textarea') as HTMLTextAreaElement;
    fireEvent.input(ta, { target: { value: '[1, 2, 3]' } });
    fireEvent.click(screen.getByText('应用到表单'));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('rejects invalid JSON syntax', () => {
    const onChange = vi.fn();
    render(() => <JsonAdvancedPanel config={{ a: 1 }} onChange={onChange} />);
    const ta = document.querySelector('textarea') as HTMLTextAreaElement;
    fireEvent.input(ta, { target: { value: '{bad' } });
    fireEvent.click(screen.getByText('应用到表单'));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('rejects null JSON', () => {
    const onChange = vi.fn();
    render(() => <JsonAdvancedPanel config={{ a: 1 }} onChange={onChange} />);
    const ta = document.querySelector('textarea') as HTMLTextAreaElement;
    fireEvent.input(ta, { target: { value: 'null' } });
    fireEvent.click(screen.getByText('应用到表单'));
    expect(onChange).not.toHaveBeenCalled();
  });
});
