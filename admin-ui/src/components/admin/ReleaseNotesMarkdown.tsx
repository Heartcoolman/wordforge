import { For, Show, type JSX } from 'solid-js';

/**
 * 轻量 Markdown 渲染，专为 GitHub release notes 设计。覆盖：
 * - `## H2` / `### H3` / `#### H4` 标题
 * - 无序列表 `- ` / `* `（连续行聚合为 `<ul>`）
 * - 有序列表 `1. `（连续行聚合为 `<ol>`）
 * - ``` 围栏代码块（多行原样保留）
 * - 段落（空行分段）
 * - 行内 `**bold**`、`` `code` ``、`[label](url)` 链接（http/https only）
 *
 * 不引入 marked/dompurify 等依赖；children 全部走 Solid 的文本节点 + 元素拼装，
 * 不使用 innerHTML，零 XSS 风险。GitHub release notes 多由 maintainer 撰写，
 * 内容相对可控；超出本 parser 能力的语法（表格 / HTML inline / 图片）会按原样
 * 显示在 `<p>` 里，对运维场景足够。
 */
interface Props {
  content: string;
}

type Block =
  | { type: 'h2' | 'h3' | 'h4' | 'p'; text: string }
  | { type: 'ul' | 'ol'; items: string[] }
  | { type: 'code'; lang: string; code: string };

function parseBlocks(src: string): Block[] {
  const blocks: Block[] = [];
  const lines = src.split('\n');
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    // 围栏代码块
    if (line.startsWith('```')) {
      const lang = line.slice(3).trim();
      const codeLines: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i].startsWith('```')) {
        codeLines.push(lines[i]);
        i += 1;
      }
      i += 1; // skip closing fence
      blocks.push({ type: 'code', lang, code: codeLines.join('\n') });
      continue;
    }
    // 标题
    if (line.startsWith('#### ')) {
      blocks.push({ type: 'h4', text: line.slice(5).trim() });
      i += 1;
      continue;
    }
    if (line.startsWith('### ')) {
      blocks.push({ type: 'h3', text: line.slice(4).trim() });
      i += 1;
      continue;
    }
    if (line.startsWith('## ')) {
      blocks.push({ type: 'h2', text: line.slice(3).trim() });
      i += 1;
      continue;
    }
    // 无序列表（连续 - 或 * 开头）
    if (/^[-*] /.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^[-*] /.test(lines[i])) {
        items.push(lines[i].slice(2));
        i += 1;
      }
      blocks.push({ type: 'ul', items });
      continue;
    }
    // 有序列表
    if (/^\d+\.\s/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\d+\.\s/.test(lines[i])) {
        items.push(lines[i].replace(/^\d+\.\s/, ''));
        i += 1;
      }
      blocks.push({ type: 'ol', items });
      continue;
    }
    // 空行 → 跳过
    if (line.trim() === '') {
      i += 1;
      continue;
    }
    // 段落：聚合到下一空行
    const paraLines: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== '' &&
      !lines[i].startsWith('```') &&
      !lines[i].startsWith('## ') &&
      !lines[i].startsWith('### ') &&
      !lines[i].startsWith('#### ') &&
      !/^[-*] /.test(lines[i]) &&
      !/^\d+\.\s/.test(lines[i])
    ) {
      paraLines.push(lines[i]);
      i += 1;
    }
    blocks.push({ type: 'p', text: paraLines.join(' ') });
  }
  return blocks;
}

/**
 * 行内 markdown → JSX 数组。处理 **bold** / `code` / [label](url)。
 * 嵌套不支持（如 **`code`**），按出现顺序贪心匹配。
 */
function renderInline(text: string): JSX.Element[] {
  const parts: JSX.Element[] = [];
  // 单次正则按顺序消化：每次匹配 4 类之一，剩余文本继续扫描
  const re = /(\*\*([^*]+)\*\*)|(`([^`]+)`)|(\[([^\]]+)\]\((https?:\/\/[^)\s]+)\))/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    if (m[1]) {
      parts.push(<strong>{m[2]}</strong>);
    } else if (m[3]) {
      parts.push(
        <code class="text-[0.85em] bg-surface-secondary/60 px-1 rounded font-mono">{m[4]}</code>,
      );
    } else if (m[5]) {
      parts.push(
        <a href={m[7]} target="_blank" rel="noopener" class="text-accent hover:underline">
          {m[6]}
        </a>,
      );
    }
    last = re.lastIndex;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts.length > 0 ? parts : [text];
}

export function ReleaseNotesMarkdown(props: Props) {
  const blocks = () => parseBlocks(props.content ?? '');
  return (
    <div class="release-notes text-sm text-content leading-relaxed space-y-2">
      <For each={blocks()}>
        {(b) => (
          <Show when={b}>
            {(_) => {
              switch (b.type) {
                case 'h2':
                  return <h3 class="text-base font-semibold text-content mt-3 mb-1">{renderInline(b.text)}</h3>;
                case 'h3':
                  return <h4 class="text-sm font-semibold text-content mt-2 mb-1">{renderInline(b.text)}</h4>;
                case 'h4':
                  return <h5 class="text-sm font-medium text-content-secondary mt-2 mb-1">{renderInline(b.text)}</h5>;
                case 'p':
                  return <p class="text-content-secondary">{renderInline(b.text)}</p>;
                case 'ul':
                  return (
                    <ul class="list-disc pl-5 text-content-secondary space-y-1">
                      <For each={b.items}>{(it) => <li>{renderInline(it)}</li>}</For>
                    </ul>
                  );
                case 'ol':
                  return (
                    <ol class="list-decimal pl-5 text-content-secondary space-y-1">
                      <For each={b.items}>{(it) => <li>{renderInline(it)}</li>}</For>
                    </ol>
                  );
                case 'code':
                  return (
                    <pre class="bg-surface-secondary/60 text-xs text-content-secondary font-mono p-2 rounded overflow-x-auto leading-relaxed">
                      <code>{b.code}</code>
                    </pre>
                  );
                default:
                  return null;
              }
            }}
          </Show>
        )}
      </For>
    </div>
  );
}
