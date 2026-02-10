import { createSignal, createEffect, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { Pagination } from '@/components/ui/Pagination';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { wordsApi } from '@/api/words';
import type { Word, CreateWordRequest } from '@/types/word';

export default function VocabularyPage() {
  const [words, setWords] = createSignal<Word[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  const [search, setSearch] = createSignal('');
  const [loading, setLoading] = createSignal(true);
  const [showForm, setShowForm] = createSignal(false);
  const [showImport, setShowImport] = createSignal(false);
  const [editing, setEditing] = createSignal<Word | null>(null);
  const pageSize = 20;

  async function load() {
    setLoading(true);
    try {
      const res = await wordsApi.list({ limit: pageSize, offset: (page() - 1) * pageSize, search: search() || undefined });
      setWords(res.items);
      setTotal(res.total);
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  onMount(load);

  function handleSearch(e: Event) {
    e.preventDefault();
    setPage(1);
    load();
  }

  function handlePageChange(p: number) {
    setPage(p);
    load();
  }

  async function deleteWord(id: string) {
    if (!confirm('确定删除该单词？')) return;
    try {
      await wordsApi.delete(id);
      uiStore.toast.success('已删除');
      load();
    } catch (err: unknown) {
      uiStore.toast.error('删除失败', err instanceof Error ? err.message : '');
    }
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <div class="flex items-center justify-between flex-wrap gap-3">
        <h1 class="text-2xl font-bold text-content">词库管理</h1>
        <div class="flex gap-2">
          <Button onClick={() => setShowImport(true)} variant="outline" size="sm">批量导入</Button>
          <Button onClick={() => { setEditing(null); setShowForm(true); }} size="sm">添加单词</Button>
        </div>
      </div>

      <form onSubmit={handleSearch} class="flex gap-2">
        <Input placeholder="搜索单词..." value={search()} onInput={(e) => setSearch(e.currentTarget.value)} class="flex-1" />
        <Button type="submit" variant="secondary">搜索</Button>
      </form>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={words().length > 0} fallback={
          <Empty title="暂无单词" description="点击添加单词或批量导入" />
        }>
          <div class="grid gap-3">
            <For each={words()}>
              {(word) => (
                <Card variant="outlined" padding="sm" class="flex items-center justify-between gap-4">
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="font-semibold text-content">{word.text}</span>
                      <Show when={word.pronunciation}>
                        <span class="text-xs text-content-tertiary">{word.pronunciation}</span>
                      </Show>
                      <Show when={word.partOfSpeech}>
                        <Badge size="sm" variant="accent">{word.partOfSpeech}</Badge>
                      </Show>
                    </div>
                    <p class="text-sm text-content-secondary truncate">{word.meaning}</p>
                  </div>
                  <div class="flex items-center gap-1 flex-shrink-0">
                    <For each={word.tags.slice(0, 2)}>
                      {(tag) => <Badge size="sm">{tag}</Badge>}
                    </For>
                    <button
                      onClick={() => { setEditing(word); setShowForm(true); }}
                      class="p-1.5 rounded text-content-tertiary hover:text-accent transition-colors cursor-pointer"
                    >
                      <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" /></svg>
                    </button>
                    <button
                      onClick={() => deleteWord(word.id)}
                      class="p-1.5 rounded text-content-tertiary hover:text-error transition-colors cursor-pointer"
                    >
                      <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                    </button>
                  </div>
                </Card>
              )}
            </For>
          </div>
          <div class="flex justify-between items-center">
            <p class="text-sm text-content-tertiary">共 {total()} 个单词</p>
            <Pagination page={page()} total={total()} pageSize={pageSize} onChange={handlePageChange} />
          </div>
        </Show>
      </Show>

      {/* Word Form Modal */}
      <WordFormModal open={showForm()} onClose={() => setShowForm(false)} word={editing()} onSaved={load} />

      {/* Import Modal */}
      <ImportModal open={showImport()} onClose={() => setShowImport(false)} onDone={load} />
    </div>
  );
}

// ─── Word Form Modal ───
function WordFormModal(props: { open: boolean; onClose: () => void; word: Word | null; onSaved: () => void }) {
  const [text, setText] = createSignal('');
  const [meaning, setMeaning] = createSignal('');
  const [pronunciation, setPronunciation] = createSignal('');
  const [partOfSpeech, setPartOfSpeech] = createSignal('');
  const [tags, setTags] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const isEdit = () => props.word !== null;

  // Reset form reactively when modal opens or word changes
  createEffect(() => {
    if (props.open) {
      const w = props.word;
      setText(w?.text ?? '');
      setMeaning(w?.meaning ?? '');
      setPronunciation(w?.pronunciation ?? '');
      setPartOfSpeech(w?.partOfSpeech ?? '');
      setTags(w?.tags?.join(', ') ?? '');
    }
  });

  async function handleSave() {
    if (!text().trim() || !meaning().trim()) {
      uiStore.toast.warning('请填写单词和释义');
      return;
    }
    setSaving(true);
    const data: CreateWordRequest = {
      text: text().trim(),
      meaning: meaning().trim(),
      pronunciation: pronunciation().trim() || undefined,
      partOfSpeech: partOfSpeech().trim() || undefined,
      tags: tags() ? tags().split(',').map((t) => t.trim()).filter(Boolean) : [],
    };
    try {
      if (isEdit() && props.word) {
        await wordsApi.update(props.word.id, data);
        uiStore.toast.success('单词已更新');
      } else {
        await wordsApi.create(data);
        uiStore.toast.success('单词已添加');
      }
      props.onSaved();
      props.onClose();
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : '');
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal open={props.open} onClose={props.onClose} title={isEdit() ? '编辑单词' : '添加单词'}>
      <div class="space-y-3 mt-2">
        <Input label="单词" value={text()} onInput={(e) => setText(e.currentTarget.value)} placeholder="例: abandon" />
        <Input label="释义" value={meaning()} onInput={(e) => setMeaning(e.currentTarget.value)} placeholder="例: 放弃" />
        <Input label="音标" value={pronunciation()} onInput={(e) => setPronunciation(e.currentTarget.value)} placeholder="例: /əˈbændən/" />
        <Input label="词性" value={partOfSpeech()} onInput={(e) => setPartOfSpeech(e.currentTarget.value)} placeholder="例: v." />
        <Input label="标签" value={tags()} onInput={(e) => setTags(e.currentTarget.value)} placeholder="逗号分隔，例: CET4, 高频" />
        <div class="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={props.onClose}>取消</Button>
          <Button onClick={handleSave} loading={saving()}>{isEdit() ? '更新' : '添加'}</Button>
        </div>
      </div>
    </Modal>
  );
}

// ─── Import Modal ───
function ImportModal(props: { open: boolean; onClose: () => void; onDone: () => void }) {
  const [mode, setMode] = createSignal<'url' | 'text'>('url');
  const [url, setUrl] = createSignal('');
  const [textContent, setTextContent] = createSignal('');
  const [importing, setImporting] = createSignal(false);

  async function handleImport() {
    setImporting(true);
    try {
      if (mode() === 'url') {
        if (!url().trim()) { uiStore.toast.warning('请输入 URL'); setImporting(false); return; }
        const res = await wordsApi.importUrl(url().trim());
        uiStore.toast.success(`成功导入 ${res.imported} 个单词`);
      } else {
        const lines = textContent().trim().split('\n').filter(Boolean);
        const wordsList: CreateWordRequest[] = [];
        for (const line of lines) {
          if (line.startsWith('#')) continue;
          let parts: string[];
          if (line.includes('\t')) parts = line.split('\t');
          else if (line.includes(' - ')) parts = line.split(' - ');
          else continue;
          if (parts.length >= 2) {
            wordsList.push({ text: parts[0].trim(), meaning: parts[1].trim() });
          }
        }
        if (wordsList.length === 0) { uiStore.toast.warning('未解析到有效数据'); setImporting(false); return; }
        // Batch in chunks of 50
        let total = 0;
        for (let i = 0; i < wordsList.length; i += 50) {
          const chunk = wordsList.slice(i, i + 50);
          const res = await wordsApi.batchCreate(chunk);
          total += res.count;
        }
        uiStore.toast.success(`成功导入 ${total} 个单词`);
      }
      props.onDone();
      props.onClose();
    } catch (err: unknown) {
      uiStore.toast.error('导入失败', err instanceof Error ? err.message : '');
    } finally {
      setImporting(false);
    }
  }

  return (
    <Modal open={props.open} onClose={props.onClose} title="批量导入" size="lg">
      <div class="space-y-4 mt-2">
        <div class="flex gap-2">
          <Button variant={mode() === 'url' ? 'primary' : 'ghost'} size="sm" onClick={() => setMode('url')}>URL 导入</Button>
          <Button variant={mode() === 'text' ? 'primary' : 'ghost'} size="sm" onClick={() => setMode('text')}>文本粘贴</Button>
        </div>
        <Show when={mode() === 'url'}>
          <Input label="词库文件 URL" value={url()} onInput={(e) => setUrl(e.currentTarget.value)} placeholder="https://raw.githubusercontent.com/..." hint="支持 Tab 分隔或 ' - ' 分隔格式" />
        </Show>
        <Show when={mode() === 'text'}>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-content-secondary">粘贴内容</label>
            <textarea
              class="w-full px-3 py-2 rounded-lg text-sm bg-surface text-content border border-border focus:outline-none focus:ring-2 focus:ring-accent/30 focus:border-accent resize-y min-h-[160px] font-mono"
              placeholder={"abandon\t放弃\nresilient - 有弹性的"}
              value={textContent()}
              onInput={(e) => setTextContent(e.currentTarget.value)}
            />
            <p class="text-xs text-content-tertiary">支持 Tab 分隔或 " - " 分隔，# 开头为注释</p>
          </div>
        </Show>
        <div class="flex justify-end gap-2">
          <Button variant="ghost" onClick={props.onClose}>取消</Button>
          <Button onClick={handleImport} loading={importing()}>开始导入</Button>
        </div>
      </div>
    </Modal>
  );
}
