import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { ReleaseNotesMarkdown } from '@/components/admin/ReleaseNotesMarkdown';

describe('ReleaseNotesMarkdown', () => {
  it('renders ## as h3 heading', () => {
    render(() => <ReleaseNotesMarkdown content={"## 升级验证记录\n\n正文"} />);
    const heading = screen.getByRole('heading', { level: 3, name: '升级验证记录' });
    expect(heading).toBeInTheDocument();
  });

  it('renders ### as h4 heading', () => {
    render(() => <ReleaseNotesMarkdown content={"### 备注"} />);
    const heading = screen.getByRole('heading', { level: 4, name: '备注' });
    expect(heading).toBeInTheDocument();
  });

  it('aggregates consecutive `-` lines into a single ul', () => {
    render(() => <ReleaseNotesMarkdown content={"- 第一项\n- 第二项\n- 第三项"} />);
    const lists = document.querySelectorAll('ul');
    expect(lists.length).toBe(1);
    expect(lists[0].querySelectorAll('li').length).toBe(3);
  });

  it('aggregates `1. ` lines into an ol', () => {
    render(() => <ReleaseNotesMarkdown content={"1. 第一步\n2. 第二步"} />);
    const ol = document.querySelector('ol');
    expect(ol).not.toBeNull();
    expect(ol!.querySelectorAll('li').length).toBe(2);
  });

  it('renders code fence as <pre><code>', () => {
    render(() => <ReleaseNotesMarkdown content={"```bash\nsystemctl restart wordforge\n```"} />);
    const code = document.querySelector('pre code');
    expect(code).not.toBeNull();
    expect(code!.textContent).toBe('systemctl restart wordforge');
  });

  it('renders **bold** inline', () => {
    render(() => <ReleaseNotesMarkdown content={"见 **重要** 提示"} />);
    const strong = document.querySelector('strong');
    expect(strong).not.toBeNull();
    expect(strong!.textContent).toBe('重要');
  });

  it('renders `inline code`', () => {
    render(() => <ReleaseNotesMarkdown content={"路径 `/opt/wordforge/.env` 改一下"} />);
    const code = document.querySelector('p code');
    expect(code).not.toBeNull();
    expect(code!.textContent).toBe('/opt/wordforge/.env');
  });

  it('renders [text](https://url) as link with target=_blank', () => {
    render(() => <ReleaseNotesMarkdown content={"看 [PR](https://github.com/x/y/pull/57)"} />);
    const link = document.querySelector('a');
    expect(link).not.toBeNull();
    expect(link!.getAttribute('href')).toBe('https://github.com/x/y/pull/57');
    expect(link!.getAttribute('target')).toBe('_blank');
    expect(link!.textContent).toBe('PR');
  });

  it('rejects non-http links（防恶意 javascript: 协议）', () => {
    render(() => <ReleaseNotesMarkdown content={"bad [click](javascript:alert(1))"} />);
    // 正则只匹配 https?:// 开头，恶意 link 不被解析为 <a>，原文留在段落里
    expect(document.querySelector('a')).toBeNull();
  });

  it('renders empty content without crash', () => {
    render(() => <ReleaseNotesMarkdown content={""} />);
    expect(document.querySelector('.release-notes')).not.toBeNull();
  });

  it('keeps paragraph text intact when no markdown syntax', () => {
    render(() => <ReleaseNotesMarkdown content={"这是一段纯文本说明。"} />);
    expect(screen.getByText('这是一段纯文本说明。')).toBeInTheDocument();
  });
});
