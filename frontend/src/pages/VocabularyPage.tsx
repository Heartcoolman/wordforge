import { createSignal, createEffect, Show, For, onMount, Switch, Match } from 'solid-js';
import { useNavigate } from '@solidjs/router';
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
import { contentApi } from '@/api/content';
import { wordStatesApi } from '@/api/wordStates';
import { IMPORT_BATCH_SIZE } from '@/lib/constants';
import type { Word, CreateWordRequest } from '@/types/word';
import type { WordLearningState, WordStateType } from '@/types/wordState';
import type { Etymology, Morpheme, WordContexts, ConfusionPairsResult } from '@/types/content';

const STATE_LABELS: Record<WordStateType, string> = {
  NEW: '新词',
  LEARNING: '学习中',
  REVIEWING: '复习中',
  MASTERED: '已掌握',
  FORGOTTEN: '已遗忘',
};

const STATE_VARIANTS: Record<WordStateType, 'default' | 'accent' | 'success' | 'warning' | 'error' | 'info'> = {
  NEW: 'info',
  LEARNING: 'accent',
  REVIEWING: 'warning',
  MASTERED: 'success',
  FORGOTTEN: 'error',
};

type FilterMode = 'all' | 'due';

export default function VocabularyPage() {
  const navigate = useNavigate();
  const [words, setWords] = createSignal<Word[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  const [search, setSearch] = createSignal('');
  const [loading, setLoading] = createSignal(true);
  const [searchLoading, setSearchLoading] = createSignal(false);
  const [showForm, setShowForm] = createSignal(false);
  const [showImport, setShowImport] = createSignal(false);
  const [editing, setEditing] = createSignal<Word | null>(null);
  const [deleteTarget, setDeleteTarget] = createSignal<string | null>(null);
  const [expandedId, setExpandedId] = createSignal<string | null>(null);
  const [stateMap, setStateMap] = createSignal<Record<string, WordLearningState>>({});
  const [filterMode, setFilterMode] = createSignal<FilterMode>('all');
  const [dueWords, setDueWords] = createSignal<WordLearningState[]>([]);
  const [dueWordDetails, setDueWordDetails] = createSignal<Word[]>([]);
  const [semanticMode, setSemanticMode] = createSignal(false);
  const canManageSystemWords = false;
  const pageSize = 20;

  async function load() {
    setLoading(true);
    try {
      const res = await wordsApi.list({ perPage: pageSize, page: page(), search: search() || undefined });
      setWords(res.data);
      setTotal(res.total);
      fetchStates(res.data.map(w => w.id));
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  async function fetchStates(wordIds: string[]) {
    if (wordIds.length === 0) return;
    try {
      const states = await wordStatesApi.batchGet(wordIds);
      const map: Record<string, WordLearningState> = { ...stateMap() };
      for (const s of states) map[s.wordId] = s;
      setStateMap(map);
    } catch {
      // Backend may not have states for these words yet
    }
  }

  async function loadDueList() {
    setLoading(true);
    try {
      const due = await wordStatesApi.getDueList();
      setDueWords(due);
      if (due.length > 0) {
        const ids = due.map(d => d.wordId);
        // Fetch word details for due words via individual lookups
        const details: Word[] = [];
        for (const id of ids) {
          try {
            const w = await wordsApi.get(id);
            details.push(w);
          } catch { /* word may have been deleted */ }
        }
        setDueWordDetails(details);
        const map: Record<string, WordLearningState> = { ...stateMap() };
        for (const s of due) map[s.wordId] = s;
        setStateMap(map);
      } else {
        setDueWordDetails([]);
      }
    } catch (err: unknown) {
      uiStore.toast.error('加载待复习列表失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  onMount(load);

  function handleSearch(e: Event) {
    e.preventDefault();
    if (searchLoading()) return;
    setSearchLoading(true);
    if (filterMode() === 'due') {
      setFilterMode('all');
    }

    if (semanticMode() && search().trim()) {
      contentApi.semanticSearch(search().trim())
        .then(res => {
          setPage(1);
          setWords(res.results as Word[]);
          setTotal(res.total);
          fetchStates(res.results.map(w => w.id));
        })
        .catch((err: unknown) => {
          uiStore.toast.error('语义搜索失败，回退到普通搜索', err instanceof Error ? err.message : '');
          setPage(1);
          return load();
        })
        .finally(() => setSearchLoading(false));
    } else {
      setPage(1);
      load().finally(() => setSearchLoading(false));
    }
  }

  function handlePageChange(p: number) {
    setPage(p);
    load();
  }

  function handleFilterChange(mode: FilterMode) {
    setFilterMode(mode);
    setExpandedId(null);
    if (mode === 'due') {
      loadDueList();
    } else {
      load();
    }
  }

  async function deleteWord(id: string) {
    try {
      await wordsApi.delete(id);
      uiStore.toast.success('已删除');
      if (filterMode() === 'due') loadDueList(); else load();
    } catch (err: unknown) {
      uiStore.toast.error('删除失败', err instanceof Error ? err.message : '');
    } finally {
      setDeleteTarget(null);
    }
  }

  function confirmDelete(id: string) {
    setDeleteTarget(id);
  }

  async function handleMarkMastered(wordId: string) {
    try {
      const updated = await wordStatesApi.markMastered(wordId);
      setStateMap(prev => ({ ...prev, [wordId]: updated }));
      uiStore.toast.success('已标记为掌握');
    } catch (err: unknown) {
      uiStore.toast.error('操作失败', err instanceof Error ? err.message : '');
    }
  }

  async function handleResetState(wordId: string) {
    try {
      const updated = await wordStatesApi.reset(wordId);
      setStateMap(prev => ({ ...prev, [wordId]: updated }));
      uiStore.toast.success('状态已重置');
    } catch (err: unknown) {
      uiStore.toast.error('操作失败', err instanceof Error ? err.message : '');
    }
  }

  function toggleExpand(wordId: string) {
    setExpandedId(prev => prev === wordId ? null : wordId);
  }

  const displayWords = () => filterMode() === 'due' ? dueWordDetails() : words();

  return (
    <div class="space-y-6 animate-fade-in-up">
      <div class="flex items-center justify-between flex-wrap gap-3">
        <h1 class="text-2xl font-bold text-content">词库管理</h1>
        <div class="flex gap-2">
          <Button onClick={() => navigate('/wordbooks')} variant="ghost" size="sm">去个人词书</Button>
          <Show when={canManageSystemWords}>
            <Button onClick={() => setShowImport(true)} variant="outline" size="sm">批量导入</Button>
            <Button onClick={() => { setEditing(null); setShowForm(true); }} size="sm">添加单词</Button>
          </Show>
        </div>
      </div>

      <form onSubmit={handleSearch} class="flex gap-2">
        <Input placeholder={semanticMode() ? '语义搜索...' : '搜索单词...'} value={search()} onInput={(e) => setSearch(e.currentTarget.value)} class="flex-1" />
        <Button
          type="button"
          variant={semanticMode() ? 'primary' : 'ghost'}
          size="sm"
          onClick={() => setSemanticMode(!semanticMode())}
          title="切换语义搜索"
        >
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9.663 17h4.674M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" /></svg>
        </Button>
        <Button type="submit" variant="secondary" loading={searchLoading()}>搜索</Button>
      </form>

      {/* Filter tabs */}
      <div class="flex gap-2">
        <Button
          variant={filterMode() === 'all' ? 'primary' : 'ghost'}
          size="xs"
          onClick={() => handleFilterChange('all')}
        >全部</Button>
        <Button
          variant={filterMode() === 'due' ? 'warning' : 'ghost'}
          size="xs"
          onClick={() => handleFilterChange('due')}
        >待复习</Button>
      </div>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={displayWords().length > 0} fallback={
          <Empty
            title={filterMode() === 'due' ? '暂无待复习单词' : '暂无单词'}
            description={filterMode() === 'due' ? '所有单词都已复习' : (canManageSystemWords ? '点击添加单词或批量导入' : '系统词库为只读，请到个人词书管理写入')}
          />
        }>
          <div class="grid gap-3">
            <For each={displayWords()}>
              {(word) => {
                const ws = () => stateMap()[word.id];
                return (
                  <div>
                    <Card
                      variant="outlined"
                      padding="sm"
                      class="flex items-center justify-between gap-4 cursor-pointer"
                      onClick={() => toggleExpand(word.id)}
                    >
                      <div class="flex-1 min-w-0">
                        <div class="flex items-center gap-2 flex-wrap">
                          <span class="font-semibold text-content">{word.text}</span>
                          <Show when={word.pronunciation}>
                            <span class="text-xs text-content-tertiary">{word.pronunciation}</span>
                          </Show>
                          <Show when={word.partOfSpeech}>
                            <Badge size="sm" variant="accent">{word.partOfSpeech}</Badge>
                          </Show>
                          <Show when={ws()}>
                            <Badge size="sm" variant={STATE_VARIANTS[ws()!.state]}>{STATE_LABELS[ws()!.state]}</Badge>
                          </Show>
                        </div>
                        <p class="text-sm text-content-secondary truncate">{word.meaning}</p>
                      </div>
                      <div class="flex items-center gap-1 flex-shrink-0" onClick={(e) => e.stopPropagation()}>
                        <For each={word.tags.slice(0, 2)}>
                          {(tag) => <Badge size="sm">{tag}</Badge>}
                        </For>
                        <Show when={ws()?.state !== 'MASTERED'}>
                          <button
                            onClick={() => handleMarkMastered(word.id)}
                            class="p-1.5 rounded text-content-tertiary hover:text-success transition-colors cursor-pointer"
                            title="标记为已掌握"
                          >
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" /></svg>
                          </button>
                        </Show>
                        <Show when={ws() && ws()!.state !== 'NEW'}>
                          <button
                            onClick={() => handleResetState(word.id)}
                            class="p-1.5 rounded text-content-tertiary hover:text-warning transition-colors cursor-pointer"
                            title="重置状态"
                          >
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" /></svg>
                          </button>
                        </Show>
                        <Show when={canManageSystemWords}>
                          <button
                            onClick={() => { setEditing(word); setShowForm(true); }}
                            class="p-1.5 rounded text-content-tertiary hover:text-accent transition-colors cursor-pointer"
                          >
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" /></svg>
                          </button>
                          <button
                            onClick={() => confirmDelete(word.id)}
                            class="p-1.5 rounded text-content-tertiary hover:text-error transition-colors cursor-pointer"
                          >
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" /></svg>
                          </button>
                        </Show>
                      </div>
                    </Card>
                    <Show when={expandedId() === word.id}>
                      <WordDetailPanel wordId={word.id} wordText={word.text} />
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
          <Show when={filterMode() === 'all'}>
            <div class="flex justify-between items-center">
              <p class="text-sm text-content-tertiary">共 {total()} 个单词</p>
              <Pagination page={page()} total={total()} pageSize={pageSize} onChange={handlePageChange} />
            </div>
          </Show>
          <Show when={filterMode() === 'due'}>
            <p class="text-sm text-content-tertiary">共 {dueWordDetails().length} 个待复习</p>
          </Show>
        </Show>
      </Show>

      <Show when={canManageSystemWords}>
        {/* Word Form Modal */}
        <WordFormModal open={showForm()} onClose={() => setShowForm(false)} word={editing()} onSaved={() => { if (filterMode() === 'due') loadDueList(); else load(); }} />

        {/* Import Modal */}
        <ImportModal open={showImport()} onClose={() => setShowImport(false)} onDone={() => { if (filterMode() === 'due') loadDueList(); else load(); }} />

        {/* Delete Confirm Modal */}
        <Modal open={deleteTarget() !== null} onClose={() => setDeleteTarget(null)} title="确认删除" size="sm">
          <p class="text-sm text-content-secondary mt-2">确定要删除该单词吗？此操作无法撤销。</p>
          <div class="flex justify-end gap-2 mt-4">
            <Button variant="ghost" onClick={() => setDeleteTarget(null)}>取消</Button>
            <Button variant="danger" onClick={() => { const id = deleteTarget(); if (id) deleteWord(id); }}>删除</Button>
          </div>
        </Modal>
      </Show>
    </div>
  );
}

// ─── Word Detail Panel (inline expansion) ───
function WordDetailPanel(props: { wordId: string; wordText: string }) {
  const [tab, setTab] = createSignal<'etymology' | 'morphemes' | 'confusion' | 'contexts'>('etymology');
  const [etymology, setEtymology] = createSignal<Etymology | null>(null);
  const [morphemes, setMorphemes] = createSignal<Morpheme[]>([]);
  const [confusionPairs, setConfusionPairs] = createSignal<ConfusionPairsResult | null>(null);
  const [wordContexts, setWordContexts] = createSignal<WordContexts | null>(null);
  const [detailLoading, setDetailLoading] = createSignal(false);
  const [detailError, setDetailError] = createSignal('');

  function loadTab(t: typeof tab extends () => infer R ? R : never) {
    setDetailLoading(true);
    setDetailError('');

    const fetcher = (() => {
      switch (t) {
        case 'etymology':
          return contentApi.getEtymology(props.wordId).then(res => setEtymology(res));
        case 'morphemes':
          return contentApi.getMorphemes(props.wordId).then(res => setMorphemes(res.morphemes));
        case 'confusion':
          return contentApi.getConfusionPairs(props.wordId).then(res => setConfusionPairs(res));
        case 'contexts':
          return contentApi.getWordContexts(props.wordId).then(res => setWordContexts(res));
      }
    })();

    fetcher
      .catch((err: unknown) => setDetailError(err instanceof Error ? err.message : '加载失败'))
      .finally(() => setDetailLoading(false));
  }

  onMount(() => loadTab('etymology'));

  function switchTab(t: typeof tab extends () => infer R ? R : never) {
    setTab(t);
    loadTab(t);
  }

  const MORPHEME_TYPE_LABELS: Record<string, string> = { prefix: '前缀', root: '词根', suffix: '后缀' };

  return (
    <div class="ml-4 mr-1 mt-1 mb-2 p-4 bg-surface-secondary rounded-lg border border-border/50 space-y-3 animate-fade-in-up">
      <div class="flex gap-1 flex-wrap">
        <Button size="xs" variant={tab() === 'etymology' ? 'primary' : 'ghost'} onClick={() => switchTab('etymology')}>词源</Button>
        <Button size="xs" variant={tab() === 'morphemes' ? 'primary' : 'ghost'} onClick={() => switchTab('morphemes')}>构词</Button>
        <Button size="xs" variant={tab() === 'confusion' ? 'primary' : 'ghost'} onClick={() => switchTab('confusion')}>易混淆</Button>
        <Button size="xs" variant={tab() === 'contexts' ? 'primary' : 'ghost'} onClick={() => switchTab('contexts')}>语境</Button>
      </div>

      <Show when={detailLoading()}>
        <div class="flex justify-center py-4"><Spinner size="sm" /></div>
      </Show>

      <Show when={detailError()}>
        <p class="text-sm text-error">{detailError()}</p>
      </Show>

      <Show when={!detailLoading() && !detailError()}>
        <Switch>
          <Match when={tab() === 'etymology'}>
            <Show when={etymology()} fallback={<p class="text-sm text-content-tertiary">暂无词源数据</p>}>
              <div class="space-y-2">
                <p class="text-sm text-content">{etymology()!.etymology}</p>
                <Show when={etymology()!.roots.length > 0}>
                  <div class="flex gap-1 flex-wrap">
                    <span class="text-xs text-content-tertiary">词根:</span>
                    <For each={etymology()!.roots}>
                      {(root) => <Badge size="sm" variant="accent">{root}</Badge>}
                    </For>
                  </div>
                </Show>
                <Show when={!etymology()!.generated}>
                  <p class="text-xs text-content-tertiary italic">当前为规则化解释，后续可由 LLM 结果覆盖</p>
                </Show>
              </div>
            </Show>
          </Match>

          <Match when={tab() === 'morphemes'}>
            <Show when={morphemes().length > 0} fallback={<p class="text-sm text-content-tertiary">暂无构词数据</p>}>
              <div class="flex gap-2 flex-wrap">
                <For each={morphemes()}>
                  {(m) => (
                    <div class="px-3 py-1.5 bg-surface rounded-lg border border-border/50">
                      <span class="font-mono font-semibold text-sm text-content">{m.text}</span>
                      <span class="text-xs text-content-tertiary ml-1">({MORPHEME_TYPE_LABELS[m.type] ?? m.type})</span>
                      <p class="text-xs text-content-secondary">{m.meaning}</p>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </Match>

          <Match when={tab() === 'confusion'}>
            <Show when={confusionPairs()?.confusionPairs.length} fallback={<p class="text-sm text-content-tertiary">暂无易混淆词</p>}>
              <div class="space-y-2">
                <For each={confusionPairs()!.confusionPairs}>
                  {(pair) => (
                    <div class="flex items-center justify-between px-3 py-2 bg-surface rounded-lg border border-border/50">
                      <div>
                        <span class="font-semibold text-sm text-content">{pair.word}</span>
                        <span class="text-xs text-content-secondary ml-2">{pair.meaning}</span>
                      </div>
                      <Badge size="sm" variant={pair.similarity > 0.8 ? 'error' : pair.similarity > 0.5 ? 'warning' : 'default'}>
                        {(pair.similarity * 100).toFixed(0)}%
                      </Badge>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </Match>

          <Match when={tab() === 'contexts'}>
            <Show when={wordContexts()?.examples.length} fallback={<p class="text-sm text-content-tertiary">暂无语境例句</p>}>
              <div class="space-y-2">
                <For each={wordContexts()!.examples}>
                  {(example) => (
                    <p class="text-sm text-content pl-3 border-l-2 border-accent/30">{example}</p>
                  )}
                </For>
              </div>
            </Show>
          </Match>
        </Switch>
      </Show>
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
        const trimmedUrl = url().trim();
        if (!trimmedUrl) { uiStore.toast.warning('请输入 URL'); setImporting(false); return; }
        if (!trimmedUrl.startsWith('https://')) {
          uiStore.toast.warning('仅支持 https:// 开头的 URL');
          setImporting(false);
          return;
        }
        const res = await wordsApi.importUrl(trimmedUrl);
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
        for (let i = 0; i < wordsList.length; i += IMPORT_BATCH_SIZE) {
          const chunk = wordsList.slice(i, i + IMPORT_BATCH_SIZE);
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
