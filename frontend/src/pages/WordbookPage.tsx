import { createSignal, createEffect, Show, For, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { Input } from '@/components/ui/Input';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { Pagination } from '@/components/ui/Pagination';
import { uiStore } from '@/stores/ui';
import { wordbooksApi } from '@/api/wordbooks';
import { wordsApi } from '@/api/words';
import { studyConfigApi } from '@/api/studyConfig';
import type { Wordbook } from '@/types/wordbook';
import type { Word } from '@/types/word';

export default function WordbookPage() {
  const navigate = useNavigate();
  const [systemBooks, setSystemBooks] = createSignal<Wordbook[]>([]);
  const [userBooks, setUserBooks] = createSignal<Wordbook[]>([]);
  const [selectedIds, setSelectedIds] = createSignal<string[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [showCreate, setShowCreate] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [detailBook, setDetailBook] = createSignal<Wordbook | null>(null);

  async function load() {
    setLoading(true);
    try {
      const [sys, usr, config] = await Promise.all([
        wordbooksApi.getSystem(),
        wordbooksApi.getUser(),
        studyConfigApi.get(),
      ]);
      setSystemBooks(sys);
      setUserBooks(usr);
      setSelectedIds(config.selectedWordbookIds || []);
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  onMount(load);

  async function toggleSelect(id: string) {
    if (saving()) return;
    const current = selectedIds();
    const next = current.includes(id) ? current.filter((x) => x !== id) : [...current, id];
    setSelectedIds(next);
    setSaving(true);
    try {
      await studyConfigApi.update({ selectedWordbookIds: next });
      uiStore.toast.success('词书配置已更新');
    } catch {
      setSelectedIds(current);
      uiStore.toast.error('更新失败');
    } finally {
      setSaving(false);
    }
  }

  function BookCard(props: { book: Wordbook }) {
    const isSelected = () => selectedIds().includes(props.book.id);
    const isUser = () => props.book.type === 'user';
    return (
      <Card
        variant={isSelected() ? 'filled' : 'outlined'}
        hover={!saving()}
        padding="md"
        onClick={() => toggleSelect(props.book.id)}
        class={`${isSelected() ? 'ring-2 ring-accent' : ''} ${saving() ? 'opacity-60 pointer-events-none' : ''}`}
        role="button"
        tabIndex={0}
        aria-label={`${props.book.name}${isSelected() ? '（已选）' : ''}`}
        onKeyDown={(e: KeyboardEvent) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            toggleSelect(props.book.id);
          }
        }}
      >
        <div class="flex items-start justify-between">
          <div>
            <h3 class="font-semibold text-content">{props.book.name}</h3>
            <Show when={props.book.description}>
              <p class="text-sm text-content-secondary mt-1">{props.book.description}</p>
            </Show>
          </div>
          <Badge variant={isUser() ? 'accent' : 'info'} size="sm">
            {isUser() ? '自定义' : '系统'}
          </Badge>
        </div>
        <div class="flex items-center justify-between mt-3">
          <div class="flex items-center gap-3 text-xs text-content-tertiary">
            <span>{props.book.wordCount} 个单词</span>
            <Show when={isSelected()}>
              <Badge variant="success" size="sm">已选</Badge>
            </Show>
          </div>
          <Button
            size="xs"
            variant="ghost"
            onClick={(e: MouseEvent) => { e.stopPropagation(); setDetailBook(props.book); }}
          >
            查看词汇
          </Button>
        </div>
      </Card>
    );
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-content">词书管理</h1>
        <div class="flex gap-2">
          <Button onClick={() => navigate('/wordbook-center')} size="sm" variant="ghost">词书中心</Button>
          <Button onClick={() => setShowCreate(true)} size="sm">创建词书</Button>
        </div>
      </div>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={systemBooks().length > 0}>
          <div>
            <h2 class="text-lg font-semibold text-content mb-3">系统词书</h2>
            <div class="grid sm:grid-cols-2 gap-3">
              <For each={systemBooks()}>{(b) => <BookCard book={b} />}</For>
            </div>
          </div>
        </Show>

        <div>
          <h2 class="text-lg font-semibold text-content mb-3">我的词书</h2>
          <Show when={userBooks().length > 0} fallback={
            <Empty title="还没有自定义词书" description="创建自己的词书来组织单词" action={<Button onClick={() => setShowCreate(true)} size="sm">创建词书</Button>} />
          }>
            <div class="grid sm:grid-cols-2 gap-3">
              <For each={userBooks()}>{(b) => <BookCard book={b} />}</For>
            </div>
          </Show>
        </div>

        <Show when={selectedIds().length > 0}>
          <p class="text-sm text-content-secondary">已选择 {selectedIds().length} 本词书用于学习</p>
        </Show>
      </Show>

      <CreateBookModal open={showCreate()} onClose={() => setShowCreate(false)} onCreated={load} />
      <WordbookDetailModal
        book={detailBook()}
        onClose={() => setDetailBook(null)}
        onChanged={load}
      />
    </div>
  );
}

function CreateBookModal(props: { open: boolean; onClose: () => void; onCreated: () => void }) {
  const [name, setName] = createSignal('');
  const [desc, setDesc] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  async function handleCreate() {
    if (!name().trim()) { uiStore.toast.warning('请输入词书名称'); return; }
    setSaving(true);
    try {
      await wordbooksApi.create({ name: name().trim(), description: desc().trim() || undefined });
      uiStore.toast.success('词书已创建');
      setName(''); setDesc('');
      props.onCreated();
      props.onClose();
    } catch (err: unknown) {
      uiStore.toast.error('创建失败', err instanceof Error ? err.message : '');
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal open={props.open} onClose={props.onClose} title="创建词书">
      <div class="space-y-3 mt-2">
        <Input label="名称" value={name()} onInput={(e) => setName(e.currentTarget.value)} placeholder="例: GRE 核心词汇" />
        <Input label="描述" value={desc()} onInput={(e) => setDesc(e.currentTarget.value)} placeholder="可选" />
        <div class="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={props.onClose}>取消</Button>
          <Button onClick={handleCreate} loading={saving()}>创建</Button>
        </div>
      </div>
    </Modal>
  );
}

const PAGE_SIZE = 20;

function WordbookDetailModal(props: { book: Wordbook | null; onClose: () => void; onChanged: () => void }) {
  const [words, setWords] = createSignal<Word[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  const [loadingWords, setLoadingWords] = createSignal(false);
  const [removingId, setRemovingId] = createSignal<string | null>(null);
  const [confirmRemoveId, setConfirmRemoveId] = createSignal<string | null>(null);
  const [showAddWords, setShowAddWords] = createSignal(false);

  const isUser = () => props.book?.type === 'user';

  async function loadWords(p = 1) {
    if (!props.book) return;
    setLoadingWords(true);
    try {
      const res = await wordbooksApi.getWords(props.book.id, { page: p, perPage: PAGE_SIZE });
      setWords(res.data);
      setTotal(res.total);
      setPage(p);
    } catch (err: unknown) {
      uiStore.toast.error('加载词汇失败', err instanceof Error ? err.message : '');
    } finally {
      setLoadingWords(false);
    }
  }

  createEffect(() => {
    const book = props.book;
    if (book) {
      setPage(1);
      setWords([]);
      setTotal(0);
      setConfirmRemoveId(null);
      setShowAddWords(false);
      loadWords(1);
    }
  });

  async function handleRemove(wordId: string) {
    if (!props.book || removingId()) return;
    setRemovingId(wordId);
    try {
      await wordbooksApi.removeWord(props.book.id, wordId);
      uiStore.toast.success('已移除');
      setConfirmRemoveId(null);
      props.onChanged();
      await loadWords(page());
    } catch (err: unknown) {
      uiStore.toast.error('移除失败', err instanceof Error ? err.message : '');
    } finally {
      setRemovingId(null);
    }
  }

  return (
    <Modal open={!!props.book} onClose={props.onClose} title={props.book?.name ?? ''} size="lg">
      <div class="space-y-4 mt-1">
        <Show when={props.book?.description}>
          <p class="text-sm text-content-secondary">{props.book!.description}</p>
        </Show>

        <div class="flex items-center justify-between">
          <span class="text-sm text-content-tertiary">共 {total()} 个单词</span>
          <Show when={isUser()}>
            <Button size="xs" onClick={() => setShowAddWords(true)}>添加单词</Button>
          </Show>
        </div>

        <Show when={loadingWords()}>
          <div class="flex justify-center py-8"><Spinner /></div>
        </Show>

        <Show when={!loadingWords() && words().length === 0}>
          <Empty title="暂无单词" description={isUser() ? '点击上方按钮添加单词' : ''} />
        </Show>

        <Show when={!loadingWords() && words().length > 0}>
          <div class="space-y-1.5 max-h-[50vh] overflow-y-auto">
            <For each={words()}>
              {(word) => (
                <div class="flex items-center justify-between px-3 py-2 rounded-lg hover:bg-surface-secondary transition-colors group">
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <span class="font-medium text-content">{word.text}</span>
                      <Show when={word.pronunciation}>
                        <span class="text-xs text-content-tertiary">/{word.pronunciation}/</span>
                      </Show>
                      <Show when={word.partOfSpeech}>
                        <Badge variant="default" size="sm">{word.partOfSpeech}</Badge>
                      </Show>
                    </div>
                    <p class="text-sm text-content-secondary truncate mt-0.5">{word.meaning}</p>
                  </div>
                  <Show when={isUser()}>
                    <Show when={confirmRemoveId() === word.id} fallback={
                      <Button
                        size="xs"
                        variant="ghost"
                        class="opacity-0 group-hover:opacity-100 transition-opacity text-content-tertiary hover:text-error"
                        onClick={() => setConfirmRemoveId(word.id)}
                      >
                        移除
                      </Button>
                    }>
                      <div class="flex items-center gap-1">
                        <Button
                          size="xs"
                          variant="danger"
                          loading={removingId() === word.id}
                          onClick={() => handleRemove(word.id)}
                        >
                          确认
                        </Button>
                        <Button
                          size="xs"
                          variant="ghost"
                          onClick={() => setConfirmRemoveId(null)}
                        >
                          取消
                        </Button>
                      </div>
                    </Show>
                  </Show>
                </div>
              )}
            </For>
          </div>
          <Pagination page={page()} total={total()} pageSize={PAGE_SIZE} onChange={loadWords} />
        </Show>
      </div>

      <Show when={showAddWords() && props.book}>
        <AddWordsModal
          bookId={props.book!.id}
          existingWordIds={words().map((w) => w.id)}
          onClose={() => setShowAddWords(false)}
          onAdded={() => { loadWords(page()); props.onChanged(); }}
        />
      </Show>
    </Modal>
  );
}

function AddWordsModal(props: {
  bookId: string;
  existingWordIds: string[];
  onClose: () => void;
  onAdded: () => void;
}) {
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<Word[]>([]);
  const [searching, setSearching] = createSignal(false);
  const [adding, setAdding] = createSignal(false);
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  const [searched, setSearched] = createSignal(false);

  let debounceTimer: ReturnType<typeof setTimeout>;

  function handleInput(value: string) {
    setQuery(value);
    clearTimeout(debounceTimer);
    if (!value.trim()) {
      setResults([]);
      setSearched(false);
      return;
    }
    debounceTimer = setTimeout(() => search(value.trim()), 300);
  }

  async function search(q: string) {
    setSearching(true);
    try {
      const res = await wordsApi.list({ search: q, perPage: 30 });
      setResults(res.data.filter((w) => !props.existingWordIds.includes(w.id)));
      setSearched(true);
    } catch {
      uiStore.toast.error('搜索失败');
    } finally {
      setSearching(false);
    }
  }

  function toggleWord(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }

  async function handleAdd() {
    const ids = Array.from(selected());
    if (ids.length === 0) { uiStore.toast.warning('请选择要添加的单词'); return; }
    setAdding(true);
    try {
      const res = await wordbooksApi.addWords(props.bookId, ids);
      uiStore.toast.success(`已添加 ${res.added} 个单词`);
      setSelected(new Set());
      setQuery('');
      setResults([]);
      setSearched(false);
      props.onAdded();
      props.onClose();
    } catch (err: unknown) {
      uiStore.toast.error('添加失败', err instanceof Error ? err.message : '');
    } finally {
      setAdding(false);
    }
  }

  return (
    <Modal open onClose={props.onClose} title="添加单词" size="md">
      <div class="space-y-3 mt-2">
        <Input
          placeholder="搜索单词..."
          value={query()}
          onInput={(e) => handleInput(e.currentTarget.value)}
          autofocus
        />

        <Show when={searching()}>
          <div class="flex justify-center py-4"><Spinner /></div>
        </Show>

        <Show when={!searching() && searched() && results().length === 0}>
          <p class="text-sm text-content-tertiary text-center py-4">未找到匹配的单词</p>
        </Show>

        <Show when={!searching() && results().length > 0}>
          <div class="max-h-[40vh] overflow-y-auto space-y-1">
            <For each={results()}>
              {(word) => {
                const isChecked = () => selected().has(word.id);
                return (
                  <div
                    class={`flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer transition-colors ${isChecked() ? 'bg-accent/10' : 'hover:bg-surface-secondary'}`}
                    onClick={() => toggleWord(word.id)}
                    role="checkbox"
                    aria-checked={isChecked()}
                    tabIndex={0}
                    onKeyDown={(e: KeyboardEvent) => {
                      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleWord(word.id); }
                    }}
                  >
                    <div class={`w-4 h-4 rounded border flex-shrink-0 flex items-center justify-center transition-colors ${isChecked() ? 'bg-accent border-accent' : 'border-border'}`}>
                      <Show when={isChecked()}>
                        <svg class="w-3 h-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3">
                          <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                        </svg>
                      </Show>
                    </div>
                    <div class="min-w-0 flex-1">
                      <div class="flex items-center gap-2">
                        <span class="font-medium text-content">{word.text}</span>
                        <Show when={word.partOfSpeech}>
                          <Badge variant="default" size="sm">{word.partOfSpeech}</Badge>
                        </Show>
                      </div>
                      <p class="text-sm text-content-secondary truncate">{word.meaning}</p>
                    </div>
                  </div>
                );
              }}
            </For>
          </div>
        </Show>

        <div class="flex items-center justify-between pt-2">
          <span class="text-xs text-content-tertiary">
            {selected().size > 0 ? `已选 ${selected().size} 个` : ''}
          </span>
          <div class="flex gap-2">
            <Button variant="ghost" onClick={props.onClose}>取消</Button>
            <Button onClick={handleAdd} loading={adding()} disabled={selected().size === 0}>
              添加 {selected().size > 0 ? `(${selected().size})` : ''}
            </Button>
          </div>
        </div>
      </div>
    </Modal>
  );
}
