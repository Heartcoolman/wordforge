import { createSignal, createResource, createMemo, createEffect, Show, For, onMount } from 'solid-js';
import { Modal } from '@/components/ui/Modal';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Input } from '@/components/ui/Input';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import type {
  WordbookSummary, WordbookType, WordbookTypeFilter, WordbookSortKey,
  WordEntrySort, WordbookExport,
} from '@/types/adminWordbooks';
import type { BrowseItem, WordbookPreview, UpdateInfo } from '@/types/wordbookCenter';
import './wordbookCenter.css';

type DetailTab = 'words' | 'heatmap' | 'distribution' | 'history';

const COVER_CLASSES = ['c-1', 'c-2', 'c-3', 'c-4', 'c-5', 'c-6'];
function coverClass(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return COVER_CLASSES[h % COVER_CLASSES.length];
}
function coverText(name: string): string {
  const t = name.trim();
  if (!t) return '#';
  // 中文取首字;英文取前 4 字符大写
  return /[a-zA-Z0-9]/.test(t[0]) ? t.slice(0, 4).toUpperCase() : t.slice(0, 2);
}
function fmtNum(n: number): string {
  return n.toLocaleString('en-US');
}
function relTime(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const d = (Date.now() - t) / 1000;
  if (d < 60) return '刚刚';
  if (d < 3600) return `${Math.floor(d / 60)} 分钟前`;
  if (d < 86400) return `${Math.floor(d / 3600)} 小时前`;
  if (d < 86400 * 30) return `${Math.floor(d / 86400)} 天前`;
  return new Date(t).toLocaleDateString('zh-CN');
}
function fmtDate(iso: string): string {
  const t = Date.parse(iso);
  return Number.isNaN(t) ? iso : new Date(t).toLocaleDateString('zh-CN');
}
function accClass(acc?: number | null): string {
  if (acc == null) return '';
  if (acc >= 0.6) return 'acc-good';
  if (acc >= 0.45) return 'acc-mid';
  return 'acc-bad';
}
const ACTION_LABEL: Record<string, string> = {
  create: '创建', update: '编辑', delete: '删除', add_word: '添加词条',
  remove_word: '移除词条', import: '导入', sync: '同步',
};

const PER_PAGE_WORDS = 20;

export default function WordbookCenterPage() {
  // ── 左列表筛选 ──
  const [search, setSearch] = createSignal('');
  const [typeFilter, setTypeFilter] = createSignal<WordbookTypeFilter>('all');
  const [sort, setSort] = createSignal<WordbookSortKey>('hottest');
  const [selectedId, setSelectedId] = createSignal<string | null>(null);

  const [listRes, { refetch: refetchList }] = createResource(
    () => ({ search: search().trim(), type: typeFilter(), sort: sort() }),
    (q) => adminApi.adminWordbooksList({ ...q, perPage: 200 }),
  );

  const items = () => listRes()?.items ?? [];
  const counts = () => listRes()?.counts;
  const selected = createMemo(() => items().find((w) => w.id === selectedId()) ?? null);

  // 列表加载后自动选中第一项
  createEffect(() => {
    const list = items();
    if (list.length > 0 && !list.some((w) => w.id === selectedId())) {
      setSelectedId(list[0].id);
    }
  });

  // ── 右侧详情数据源(随 selectedId 变化拉取) ──
  const [statsRes] = createResource(selectedId, (id) => adminApi.adminWordbookStats(id));

  const [detailTab, setDetailTab] = createSignal<DetailTab>('words');

  // 词条 tab 过滤 + 分页
  const [wordSearch, setWordSearch] = createSignal('');
  const [wordSort, setWordSort] = createSignal<WordEntrySort>('frequency');
  const [wordPos, setWordPos] = createSignal('');
  const [wordPage, setWordPage] = createSignal(1);

  const [wordsRes] = createResource(
    () => {
      const id = selectedId();
      if (!id) return null;
      return { id, search: wordSearch().trim(), sort: wordSort(), pos: wordPos(), page: wordPage() };
    },
    (q) => adminApi.adminWordbookWords(q.id, {
      search: q.search || undefined, sort: q.sort, pos: q.pos || undefined,
      page: q.page, perPage: PER_PAGE_WORDS,
    }),
  );

  const [heatmapRes] = createResource(
    () => (detailTab() === 'heatmap' ? selectedId() : null),
    (id) => adminApi.adminWordbookHeatmap(id),
  );
  const [distRes] = createResource(
    () => (detailTab() === 'distribution' ? selectedId() : null),
    (id) => adminApi.adminWordbookDistribution(id),
  );
  const [historyRes] = createResource(
    () => (detailTab() === 'history' ? selectedId() : null),
    (id) => adminApi.adminWordbookHistory(id, { perPage: 100 }),
  );

  // 选中切换 → 重置词条 tab 状态
  function selectWordbook(id: string) {
    if (id === selectedId()) return;
    setSelectedId(id);
    setDetailTab('words');
    setWordSearch('');
    setWordPos('');
    setWordSort('frequency');
    setWordPage(1);
  }

  // ── 新建 / 编辑 / 删除 / 添加词条 modal ──
  const [createOpen, setCreateOpen] = createSignal(false);
  const [createDraft, setCreateDraft] = createSignal<{ name: string; description: string; type: WordbookType }>(
    { name: '', description: '', type: 'system' },
  );
  const [createBusy, setCreateBusy] = createSignal(false);

  const [editOpen, setEditOpen] = createSignal(false);
  const [editDraft, setEditDraft] = createSignal({ name: '', description: '' });
  const [editBusy, setEditBusy] = createSignal(false);

  const [deleteTarget, setDeleteTarget] = createSignal<WordbookSummary | null>(null);
  const [deleteBusy, setDeleteBusy] = createSignal(false);

  const [addWordOpen, setAddWordOpen] = createSignal(false);
  const [addWordDraft, setAddWordDraft] = createSignal({
    text: '', pronunciation: '', partOfSpeech: '', meaning: '', examples: '',
  });
  const [addWordBusy, setAddWordBusy] = createSignal(false);

  const [removeWordTarget, setRemoveWordTarget] = createSignal<{ id: string; text: string } | null>(null);
  const [exporting, setExporting] = createSignal(false);

  async function submitCreate() {
    const d = createDraft();
    if (!d.name.trim()) { uiStore.toast.error('词库名称必填'); return; }
    setCreateBusy(true);
    try {
      const wb = await adminApi.adminWordbookCreate({
        name: d.name.trim(), description: d.description.trim() || undefined, type: d.type,
      });
      uiStore.toast.success(`已创建「${wb.name}」`);
      setCreateOpen(false);
      setCreateDraft({ name: '', description: '', type: 'system' });
      await refetchList();
      setSelectedId(wb.id);
    } catch (err: unknown) {
      uiStore.toast.error('创建失败', err instanceof Error ? err.message : '');
    } finally {
      setCreateBusy(false);
    }
  }

  function openEdit() {
    const w = selected();
    if (!w) return;
    setEditDraft({ name: w.name, description: w.description });
    setEditOpen(true);
  }
  async function submitEdit() {
    const w = selected();
    const d = editDraft();
    if (!w) return;
    if (!d.name.trim()) { uiStore.toast.error('词库名称必填'); return; }
    setEditBusy(true);
    try {
      await adminApi.adminWordbookUpdate(w.id, { name: d.name.trim(), description: d.description.trim() });
      uiStore.toast.success('已保存');
      setEditOpen(false);
      await refetchList();
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : '');
    } finally {
      setEditBusy(false);
    }
  }

  async function confirmDelete() {
    const w = deleteTarget();
    if (!w) return;
    setDeleteBusy(true);
    try {
      await adminApi.adminWordbookDelete(w.id);
      uiStore.toast.success(`已删除「${w.name}」`);
      setDeleteTarget(null);
      if (selectedId() === w.id) setSelectedId(null);
      await refetchList();
    } catch (err: unknown) {
      uiStore.toast.error('删除失败', err instanceof Error ? err.message : '');
    } finally {
      setDeleteBusy(false);
    }
  }

  async function submitAddWord() {
    const w = selected();
    const d = addWordDraft();
    if (!w) return;
    if (!d.text.trim() || !d.meaning.trim()) { uiStore.toast.error('单词与释义必填'); return; }
    setAddWordBusy(true);
    try {
      await adminApi.adminWordbookAddWord(w.id, {
        text: d.text.trim(),
        pronunciation: d.pronunciation.trim() || undefined,
        partOfSpeech: d.partOfSpeech.trim() || undefined,
        meaning: d.meaning.trim(),
        examples: d.examples.trim() ? d.examples.split('\n').map((s) => s.trim()).filter(Boolean) : undefined,
      });
      uiStore.toast.success(`已添加「${d.text.trim()}」`);
      setAddWordOpen(false);
      setAddWordDraft({ text: '', pronunciation: '', partOfSpeech: '', meaning: '', examples: '' });
      setWordPage(1);
      await refetchList();
    } catch (err: unknown) {
      uiStore.toast.error('添加失败', err instanceof Error ? err.message : '');
    } finally {
      setAddWordBusy(false);
    }
  }

  async function confirmRemoveWord() {
    const w = selected();
    const target = removeWordTarget();
    if (!w || !target) return;
    try {
      await adminApi.adminWordbookRemoveWord(w.id, target.id);
      uiStore.toast.success(`已移除「${target.text}」`);
      setRemoveWordTarget(null);
      await refetchList();
    } catch (err: unknown) {
      uiStore.toast.error('移除失败', err instanceof Error ? err.message : '');
    }
  }

  async function exportWordbook() {
    const w = selected();
    if (!w) return;
    setExporting(true);
    try {
      const data: WordbookExport = await adminApi.adminWordbookExport(w.id);
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${w.name || w.id}.json`;
      a.click();
      URL.revokeObjectURL(url);
      uiStore.toast.success(`已导出「${w.name}」(${data.words.length} 词)`);
    } catch (err: unknown) {
      uiStore.toast.error('导出失败', err instanceof Error ? err.message : '');
    } finally {
      setExporting(false);
    }
  }

  // ── 副标题:用 counts 拼 ──
  const subtitle = createMemo(() => {
    const c = counts();
    if (!c) return '加载中…';
    return `官方词库 ${fmtNum(c.system)} 套 · 用户自建 ${fmtNum(c.user)} 套 · 共 ${fmtNum(c.totalEntries)} 条词条。可批量编辑词义 / IPA / 例句，预览答题词频热图，导入 CSV / JSON。`;
  });

  // 词频热图:把 cells 映射成 l-0..l-5 等级
  const heatLevel = (count: number, max: number): number => {
    if (max <= 0 || count <= 0) return 0;
    const r = count / max;
    if (r > 0.8) return 5;
    if (r > 0.6) return 4;
    if (r > 0.4) return 3;
    if (r > 0.2) return 2;
    if (r > 0) return 1;
    return 0;
  };

  const wordsTotalPages = () => wordsRes()?.totalPages ?? 1;

  return (
    <div class="wbc">
      {/* ── 页头 ── */}
      <div class="page-header">
        <div class="row spread wrap" style="gap:12px">
          <div>
            <h1 class="page-title">词库中心</h1>
            <p class="page-desc">{subtitle()}</p>
          </div>
          <div class="row gap-2">
            <button class="btn btn-secondary" disabled={!selected() || exporting()} onClick={exportWordbook}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
              导出
            </button>
            <button class="btn btn-primary" onClick={() => setCreateOpen(true)}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
              新建词库
            </button>
          </div>
        </div>
      </div>

      {/* ── 主体:左列表 + 右详情 ── */}
      <div class="wb-grid">
        {/* LEFT */}
        <div class="wb-list">
          <div class="wb-list-search">
            <div class="row gap-2">
              <div class="input-wrap" style="flex:1">
                <svg class="icon-leading" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/></svg>
                <input
                  class="input has-leading"
                  placeholder="搜索词库名 / 标签…"
                  value={search()}
                  onInput={(e) => setSearch(e.currentTarget.value)}
                />
              </div>
            </div>
            <div class="row gap-1 mt-2 wrap">
              <button
                class={`chip ${typeFilter() === 'all' ? 'chip-accent' : ''}`}
                style="cursor:pointer"
                onClick={() => setTypeFilter('all')}
              >
                全部 <span style="opacity:0.7;margin-left:2px;font-family:var(--font-mono);font-size:10.5px">{counts()?.all ?? '—'}</span>
              </button>
              <button
                class={`chip ${typeFilter() === 'system' ? 'chip-accent' : ''}`}
                style="cursor:pointer"
                onClick={() => setTypeFilter('system')}
              >
                官方 <span style="opacity:0.7;margin-left:2px;font-family:var(--font-mono);font-size:10.5px">{counts()?.system ?? '—'}</span>
              </button>
              <button
                class={`chip ${typeFilter() === 'user' ? 'chip-accent' : ''}`}
                style="cursor:pointer"
                onClick={() => setTypeFilter('user')}
              >
                用户 <span style="opacity:0.7;margin-left:2px;font-family:var(--font-mono);font-size:10.5px">{counts()?.user ?? '—'}</span>
              </button>
              <button
                class={`chip ${sort() === 'newest' ? 'chip-accent' : ''}`}
                style="cursor:pointer"
                onClick={() => setSort('newest')}
              >最新</button>
              <button
                class={`chip ${sort() === 'hottest' ? 'chip-accent' : ''}`}
                style="cursor:pointer"
                onClick={() => setSort('hottest')}
              >最热</button>
            </div>
          </div>
          <div class="wb-list-body">
            <Show
              when={!listRes.loading}
              fallback={<div class="row" style="justify-content:center;padding:48px 0"><Spinner size="lg" /></div>}
            >
              <Show
                when={items().length > 0}
                fallback={<Empty title="暂无词库" description="点击右上「新建词库」创建,或在下方导入远程词库" />}
              >
                <For each={items()}>
                  {(w) => (
                    <button
                      type="button"
                      class={`wb-item ${selectedId() === w.id ? 'is-selected' : ''}`}
                      onClick={() => selectWordbook(w.id)}
                      aria-label={`选择词库 ${w.name}`}
                    >
                      <div class={`cover ${coverClass(w.id)}`}>{coverText(w.name)}</div>
                      <div class="info">
                        <div class="name">{w.name}</div>
                        <div class="meta">
                          <span>{w.type === 'system' ? '官方' : '用户'}</span>
                          <Show when={w.type === 'user' && w.ownerEmail}>
                            <span class="dot" /><span>{w.ownerEmail}</span>
                          </Show>
                          <span class="dot" /><span>{fmtNum(w.wordCount)} 词</span>
                          <span class="dot" /><span>{fmtNum(w.activeUsers)} 在用</span>
                        </div>
                      </div>
                      <Show when={w.type === 'system'} fallback={<span class="chip" style="font-size:10px">用户</span>}>
                        <span class="chip chip-info" style="font-size:10px">官方</span>
                      </Show>
                    </button>
                  )}
                </For>
              </Show>
            </Show>
          </div>
        </div>

        {/* RIGHT */}
        <div class="wb-detail">
          <Show
            when={selected()}
            fallback={<Empty class="py-24" title="未选择词库" description="从左侧列表选择一个词库查看详情" />}
          >
            {(w) => (
              <>
                <div class="wb-detail-head">
                  <div class={`cover`}>{coverText(w().name)}</div>
                  <div class="info">
                    <h2>{w().name}</h2>
                    <Show when={w().description}>
                      <p class="desc">{w().description}</p>
                    </Show>
                    <div class="row gap-2 mt-2 wrap">
                      <span class={`chip ${w().type === 'system' ? 'chip-info' : ''}`}>{w().type === 'system' ? '官方' : '用户'}</span>
                      <span class="chip chip-outline">{fmtDate(w().createdAt)}</span>
                      <For each={w().tags}>
                        {(tag) => <span class="chip chip-outline">{tag}</span>}
                      </For>
                    </div>
                  </div>
                  <div class="actions">
                    <button class="btn btn-secondary" disabled={exporting()} onClick={exportWordbook}>
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
                      导出
                    </button>
                    <button class="btn btn-secondary" onClick={openEdit}>
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                      编辑
                    </button>
                    <button class="btn btn-secondary" onClick={() => setDeleteTarget(w())} style="color:var(--error-strong)">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                      删除
                    </button>
                    <button class="btn btn-primary" onClick={() => setAddWordOpen(true)}>
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>
                      添加词条
                    </button>
                  </div>
                </div>

                {/* KPI */}
                <div class="wb-stats">
                  <div><div class="l">总词条</div><div class="v">{statsRes.loading ? '—' : fmtNum(statsRes()?.totalWords ?? 0)}</div></div>
                  <div><div class="l">在用用户</div><div class="v">{statsRes.loading ? '—' : fmtNum(statsRes()?.activeUsers ?? 0)}</div></div>
                  <div><div class="l">平均掌握度</div><div class="v">{statsRes.loading ? '—' : Math.round((statsRes()?.avgMastery ?? 0) * 100)}<span class="unit">%</span></div></div>
                  <div><div class="l">本周答题</div><div class="v">{statsRes.loading ? '—' : fmtNum(statsRes()?.weeklyAnswers ?? 0)}</div></div>
                </div>

                {/* Tabs */}
                <div class="wb-tabs">
                  <button class={`wb-tab ${detailTab() === 'words' ? 'is-active' : ''}`} onClick={() => setDetailTab('words')}>
                    词条 <span class="count">{fmtNum(statsRes()?.totalWords ?? w().wordCount)}</span>
                  </button>
                  <button class={`wb-tab ${detailTab() === 'heatmap' ? 'is-active' : ''}`} onClick={() => setDetailTab('heatmap')}>
                    词频热图
                  </button>
                  <button class={`wb-tab ${detailTab() === 'distribution' ? 'is-active' : ''}`} onClick={() => setDetailTab('distribution')}>
                    用户分布
                  </button>
                  <button class={`wb-tab ${detailTab() === 'history' ? 'is-active' : ''}`} onClick={() => setDetailTab('history')}>
                    变更历史
                  </button>
                </div>

                {/* Tab: 词条 */}
                <Show when={detailTab() === 'words'}>
                  <div class="row gap-2 wrap" style="padding:14px 18px 6px">
                    <div class="input-wrap" style="max-width:280px;flex:1">
                      <svg class="icon-leading" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/></svg>
                      <input
                        class="input has-leading"
                        placeholder="搜索词条 / 释义"
                        value={wordSearch()}
                        onInput={(e) => { setWordSearch(e.currentTarget.value); setWordPage(1); }}
                      />
                    </div>
                    <select
                      class="select" style="max-width:140px"
                      value={wordSort()}
                      onChange={(e) => { setWordSort(e.currentTarget.value as WordEntrySort); setWordPage(1); }}
                    >
                      <option value="frequency">排序：词频</option>
                      <option value="alpha">排序：字母</option>
                      <option value="difficulty">排序：难度</option>
                    </select>
                    <select
                      class="select" style="max-width:120px"
                      value={wordPos()}
                      onChange={(e) => { setWordPos(e.currentTarget.value); setWordPage(1); }}
                    >
                      <option value="">词性：全部</option>
                      <option value="n.">名词</option>
                      <option value="v.">动词</option>
                      <option value="adj.">形容词</option>
                      <option value="adv.">副词</option>
                    </select>
                  </div>

                  <div class="word-list">
                    <div class="word-row-2 head">
                      <span>词 · 音标 · 词性</span>
                      <span>主要释义</span>
                      <span>例句 (示例)</span>
                      <span class="num">出现次数</span>
                      <span class="num">正确率</span>
                    </div>
                    <Show
                      when={!wordsRes.loading}
                      fallback={<div class="row" style="justify-content:center;padding:32px 0"><Spinner /></div>}
                    >
                      <Show
                        when={(wordsRes()?.data.length ?? 0) > 0}
                        fallback={<Empty title="暂无词条" description={wordSearch() ? '无匹配结果' : '点击「添加词条」录入第一个单词'} />}
                      >
                        <For each={wordsRes()?.data}>
                          {(word) => (
                            <div class="word-row-2">
                              <span class="word">
                                <strong>{word.text}</strong>
                                <Show when={word.pronunciation}><span class="ipa">{word.pronunciation}</span></Show>
                                <Show when={word.partOfSpeech}><span class="pos">{word.partOfSpeech}</span></Show>
                              </span>
                              <span class="gloss">{word.meaning}</span>
                              <span class="gloss example">{word.examples[0] ?? '—'}</span>
                              <span class="num">{fmtNum(word.appearCount)}</span>
                              <span class={`num ${accClass(word.accuracy)}`}>
                                {word.accuracy == null ? '—' : `${Math.round(word.accuracy * 100)}%`}
                                <button
                                  class="btn-icon"
                                  style="margin-left:8px;width:22px;height:22px"
                                  aria-label={`移除 ${word.text}`}
                                  title="移除词条"
                                  onClick={() => setRemoveWordTarget({ id: word.id, text: word.text })}
                                >
                                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width:13px;height:13px"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/></svg>
                                </button>
                              </span>
                            </div>
                          )}
                        </For>
                      </Show>
                    </Show>
                  </div>

                  <div class="row spread" style="padding:14px 18px;border-top:1px solid var(--border-hairline)">
                    <span class="text-small text-tertiary">
                      共 {fmtNum(wordsRes()?.total ?? 0)} 条
                    </span>
                    <div class="row gap-2">
                      <button class="btn btn-ghost btn-sm" disabled={wordPage() <= 1} onClick={() => setWordPage((p) => Math.max(1, p - 1))}>‹</button>
                      <span class="text-small tnum">{wordPage()} / {wordsTotalPages()}</span>
                      <button class="btn btn-ghost btn-sm" disabled={wordPage() >= wordsTotalPages()} onClick={() => setWordPage((p) => Math.min(wordsTotalPages(), p + 1))}>›</button>
                    </div>
                  </div>
                </Show>

                {/* Tab: 词频热图 */}
                <Show when={detailTab() === 'heatmap'}>
                  <Show
                    when={!heatmapRes.loading}
                    fallback={<div class="row" style="justify-content:center;padding:48px 0"><Spinner /></div>}
                  >
                    <Show
                      when={(heatmapRes()?.cells.length ?? 0) > 0}
                      fallback={<Empty title="暂无答题数据" description="该词库下的词暂无答题记录,无法生成热图" />}
                    >
                      <div class="freq-grid">
                        <For each={heatmapRes()?.cells}>
                          {(cell) => (
                            <div
                              class={`freq-cell l-${heatLevel(cell.count, heatmapRes()!.maxCount)}`}
                              title={`${cell.text} · ${fmtNum(cell.count)} 次`}
                            />
                          )}
                        </For>
                      </div>
                      <div class="row spread" style="padding:0 18px 14px">
                        <span class="text-small text-tertiary">每格 = 1 个词 · 颜色越深答题越频繁 · 悬停查看单词</span>
                        <div class="row gap-1" style="align-items:center">
                          <span class="text-tertiary text-mono" style="font-size:10px">低</span>
                          <span class="freq-cell l-0" style="width:14px;height:14px" />
                          <span class="freq-cell l-1" style="width:14px;height:14px" />
                          <span class="freq-cell l-2" style="width:14px;height:14px" />
                          <span class="freq-cell l-3" style="width:14px;height:14px" />
                          <span class="freq-cell l-4" style="width:14px;height:14px" />
                          <span class="freq-cell l-5" style="width:14px;height:14px" />
                          <span class="text-tertiary text-mono" style="font-size:10px">高</span>
                        </div>
                      </div>
                    </Show>
                  </Show>
                </Show>

                {/* Tab: 用户分布 */}
                <Show when={detailTab() === 'distribution'}>
                  <Show
                    when={!distRes.loading}
                    fallback={<div class="row" style="justify-content:center;padding:48px 0"><Spinner /></div>}
                  >
                    <Show
                      when={(distRes()?.totalUsers ?? 0) > 0}
                      fallback={<Empty title="暂无在用用户" description="该词库下暂无用户掌握度数据" />}
                    >
                      <div class="wb-dist">
                        <For each={distRes()?.buckets}>
                          {(b) => {
                            const total = distRes()!.totalUsers || 1;
                            const pct = (b.userCount / total) * 100;
                            return (
                              <div class="wb-dist-row">
                                <span class="l">{b.label}</span>
                                <div class="bar"><span style={`width:${pct.toFixed(1)}%`} /></div>
                                <span class="n">{fmtNum(b.userCount)} · {pct.toFixed(0)}%</span>
                              </div>
                            );
                          }}
                        </For>
                      </div>
                    </Show>
                  </Show>
                </Show>

                {/* Tab: 变更历史 */}
                <Show when={detailTab() === 'history'}>
                  <Show
                    when={!historyRes.loading}
                    fallback={<div class="row" style="justify-content:center;padding:48px 0"><Spinner /></div>}
                  >
                    <Show
                      when={(historyRes()?.data.length ?? 0) > 0}
                      fallback={<Empty title="暂无变更记录" description="对该词库的创建 / 编辑 / 导入等操作会记录在此" />}
                    >
                      <div class="wb-history">
                        <For each={historyRes()?.data}>
                          {(h) => (
                            <div class="wb-history-item">
                              <span class="chip chip-outline">{ACTION_LABEL[h.action] ?? h.action}</span>
                              <span class="detail">{h.detail || <span>—</span>}</span>
                              <span class="ts">{relTime(h.createdAt)}</span>
                            </div>
                          )}
                        </For>
                      </div>
                    </Show>
                  </Show>
                </Show>
              </>
            )}
          </Show>
        </div>
      </div>

      {/* ── 底部:导入新词库(整合远程目录 + 文件上传) ── */}
      <ImportSection onImported={refetchList} />

      {/* ── 新建 modal ── */}
      <Modal open={createOpen()} onClose={() => setCreateOpen(false)} title="新建词库" size="md">
        <div class="space-y-3">
          <Input
            label="词库名称"
            placeholder="如 CET-6 高频"
            value={createDraft().name}
            onInput={(e) => setCreateDraft({ ...createDraft(), name: e.currentTarget.value })}
          />
          <Input
            label="简介(可选)"
            placeholder="一句话说明用途"
            value={createDraft().description}
            onInput={(e) => setCreateDraft({ ...createDraft(), description: e.currentTarget.value })}
          />
          <div>
            <p class="text-sm font-medium text-content-secondary mb-1.5">类型</p>
            <div class="flex gap-2">
              <button
                type="button"
                class={`px-3 py-1.5 rounded-md text-sm border transition ${createDraft().type === 'system' ? 'border-accent text-accent bg-accent-light' : 'border-border-hairline text-content-secondary'}`}
                onClick={() => setCreateDraft({ ...createDraft(), type: 'system' })}
              >官方词库</button>
              <button
                type="button"
                class={`px-3 py-1.5 rounded-md text-sm border transition ${createDraft().type === 'user' ? 'border-accent text-accent bg-accent-light' : 'border-border-hairline text-content-secondary'}`}
                onClick={() => setCreateDraft({ ...createDraft(), type: 'user' })}
              >用户词库</button>
            </div>
          </div>
          <div class="flex justify-end gap-2 pt-1">
            <button class="btn btn-ghost" onClick={() => setCreateOpen(false)}>取消</button>
            <button class="btn btn-primary" disabled={createBusy()} onClick={submitCreate}>
              <Show when={createBusy()}><Spinner size="sm" /></Show>创建
            </button>
          </div>
        </div>
      </Modal>

      {/* ── 编辑 modal ── */}
      <Modal open={editOpen()} onClose={() => setEditOpen(false)} title="编辑词库" size="md">
        <div class="space-y-3">
          <Input
            label="词库名称"
            value={editDraft().name}
            onInput={(e) => setEditDraft({ ...editDraft(), name: e.currentTarget.value })}
          />
          <Input
            label="简介"
            value={editDraft().description}
            onInput={(e) => setEditDraft({ ...editDraft(), description: e.currentTarget.value })}
          />
          <div class="flex justify-end gap-2 pt-1">
            <button class="btn btn-ghost" onClick={() => setEditOpen(false)}>取消</button>
            <button class="btn btn-primary" disabled={editBusy()} onClick={submitEdit}>
              <Show when={editBusy()}><Spinner size="sm" /></Show>保存
            </button>
          </div>
        </div>
      </Modal>

      {/* ── 添加词条 modal ── */}
      <Modal open={addWordOpen()} onClose={() => setAddWordOpen(false)} title="添加词条" size="lg">
        <div class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <Input
              label="单词"
              placeholder="如 tenacious"
              value={addWordDraft().text}
              onInput={(e) => setAddWordDraft({ ...addWordDraft(), text: e.currentTarget.value })}
            />
            <Input
              label="音标(可选)"
              placeholder="/tɪˈneɪʃəs/"
              value={addWordDraft().pronunciation}
              onInput={(e) => setAddWordDraft({ ...addWordDraft(), pronunciation: e.currentTarget.value })}
            />
          </div>
          <div class="grid grid-cols-2 gap-3">
            <Input
              label="词性(可选)"
              placeholder="adj."
              value={addWordDraft().partOfSpeech}
              onInput={(e) => setAddWordDraft({ ...addWordDraft(), partOfSpeech: e.currentTarget.value })}
            />
            <Input
              label="释义"
              placeholder="坚韧的 / 顽强的"
              value={addWordDraft().meaning}
              onInput={(e) => setAddWordDraft({ ...addWordDraft(), meaning: e.currentTarget.value })}
            />
          </div>
          <div>
            <p class="text-sm font-medium text-content-secondary mb-1.5">例句(可选,每行一条)</p>
            <textarea
              rows={3}
              value={addWordDraft().examples}
              onInput={(e) => setAddWordDraft({ ...addWordDraft(), examples: e.currentTarget.value })}
              placeholder="She was tenacious in her pursuit of justice."
              class="w-full px-3 py-2 rounded-md border border-border-hairline bg-surface text-sm focus:outline-none focus:border-accent"
            />
          </div>
          <div class="flex justify-end gap-2 pt-1">
            <button class="btn btn-ghost" onClick={() => setAddWordOpen(false)}>取消</button>
            <button class="btn btn-primary" disabled={addWordBusy()} onClick={submitAddWord}>
              <Show when={addWordBusy()}><Spinner size="sm" /></Show>添加
            </button>
          </div>
        </div>
      </Modal>

      {/* ── 删除词库确认 ── */}
      <ConfirmDialog
        open={!!deleteTarget()}
        title="删除词库"
        message={
          <Show when={deleteTarget()}>
            {(t) => (
              <>
                将永久删除「<span class="font-medium text-content">{t().name}</span>」及其全部词条关联({fmtNum(t().wordCount)} 词)。
                <br />该操作不可撤销。
              </>
            )}
          </Show>
        }
        confirmText="确认删除"
        variant="danger"
        loading={deleteBusy()}
        onConfirm={confirmDelete}
        onCancel={() => setDeleteTarget(null)}
      />

      {/* ── 移除词条确认 ── */}
      <ConfirmDialog
        open={!!removeWordTarget()}
        title="移除词条"
        message={
          <Show when={removeWordTarget()}>
            {(t) => <>将从本词库移除「<span class="font-medium text-content">{t().text}</span>」(单词本身不删除)。</>}
          </Show>
        }
        confirmText="确认移除"
        variant="warning"
        onConfirm={confirmRemoveWord}
        onCancel={() => setRemoveWordTarget(null)}
      />
    </div>
  );
}

// ════════════ 底部「导入新词库」面板:整合远程目录浏览/导入/同步/检查更新/标签编辑 + 文件上传 ════════════

const CSV_TEMPLATE = `word,pos,ipa,gloss_zh,example
tenacious,adj.,/tɪˈneɪʃəs/,坚韧的,She was tenacious…
ephemeral,adj.,/ɪˈfɛmərəl/,短暂的,Fame can be…`;

function ImportSection(props: { onImported: () => void }) {
  const [configured, setConfigured] = createSignal(false);
  const [remoteItems, setRemoteItems] = createSignal<BrowseItem[]>([]);
  const [remoteLoading, setRemoteLoading] = createSignal(false);
  const [updates, setUpdates] = createSignal<UpdateInfo[]>([]);
  const [checking, setChecking] = createSignal(false);
  const [importing, setImporting] = createSignal<string | null>(null);
  const [syncing, setSyncing] = createSignal<string | null>(null);
  const [importTarget, setImportTarget] = createSignal<BrowseItem | null>(null);

  // 文件上传 modal
  const [uploadOpen, setUploadOpen] = createSignal(false);
  const [uploadDraft, setUploadDraft] = createSignal({ id: '', name: '', description: '', wordsJson: '[]' });
  const [uploadBusy, setUploadBusy] = createSignal(false);
  const [dragOver, setDragOver] = createSignal(false);

  // 预览 modal
  const [preview, setPreview] = createSignal<WordbookPreview | null>(null);

  // 标签编辑 modal
  const [tagsTarget, setTagsTarget] = createSignal<BrowseItem | null>(null);
  const [tagsDraft, setTagsDraft] = createSignal<string[]>([]);
  const [tagInput, setTagInput] = createSignal('');
  const [tagsBusy, setTagsBusy] = createSignal(false);

  const mutating = () => importing() !== null || syncing() !== null;

  async function loadRemote() {
    setRemoteLoading(true);
    try {
      const settings = await adminApi.getSettings();
      if (!settings.wordbookCenterUrl) {
        setConfigured(false);
        setRemoteItems([]);
        return;
      }
      setConfigured(true);
      try {
        setRemoteItems(await adminApi.wbCenterBrowse());
      } catch (err: unknown) {
        setRemoteItems([]);
        uiStore.toast.error('加载远程词库目录失败', err instanceof Error ? err.message : '');
      }
    } catch (err: unknown) {
      setConfigured(false);
      uiStore.toast.error('读取设置失败', err instanceof Error ? err.message : '');
    } finally {
      setRemoteLoading(false);
    }
  }

  onMount(loadRemote);

  async function checkUpdates() {
    setChecking(true);
    try {
      const data = await adminApi.wbCenterUpdates();
      setUpdates(data);
      if (data.length === 0) uiStore.toast.success('所有词书均为最新');
    } catch (err: unknown) {
      uiStore.toast.error('检查更新失败', err instanceof Error ? err.message : '');
    } finally {
      setChecking(false);
    }
  }

  async function handleImport(item: BrowseItem) {
    setImporting(item.id);
    try {
      const res = await adminApi.wbCenterImport(item.id);
      uiStore.toast.success(`已导入「${res.wordbook.name}」(${res.wordsImported} 词)`);
      await loadRemote();
      props.onImported();
    } catch (err: unknown) {
      uiStore.toast.error(`导入「${item.name}」失败`, err instanceof Error ? err.message : '');
    } finally {
      setImporting(null);
    }
  }

  async function handleSync(id: string, name?: string) {
    setSyncing(id);
    try {
      const res = await adminApi.wbCenterSync(id);
      uiStore.toast.success(`同步完成：新增 ${res.wordsAdded}，更新 ${res.wordsUpdated}，移除 ${res.wordsRemoved}`);
      setUpdates((prev) => prev.filter((u) => u.remoteId !== id));
      await loadRemote();
      props.onImported();
    } catch (err: unknown) {
      uiStore.toast.error(name ? `同步「${name}」失败` : '同步失败', err instanceof Error ? err.message : '');
    } finally {
      setSyncing(null);
    }
  }

  async function handlePreview(id: string) {
    try {
      setPreview(await adminApi.wbCenterPreview(id, { perPage: 20 }));
    } catch (err: unknown) {
      uiStore.toast.error('预览失败', err instanceof Error ? err.message : '');
    }
  }

  function handleFile(file: File) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const txt = e.target?.result;
      if (typeof txt !== 'string') return;
      const baseId = file.name.replace(/\.(csv|json)$/i, '');
      if (file.name.toLowerCase().endsWith('.csv')) {
        const lines = txt.split(/\r?\n/).filter((l) => l.trim());
        const words = lines.slice(1).map((line) => {
          const [spelling, phonetic, meaning] = line.split(',');
          return {
            spelling: spelling?.trim() ?? '',
            phonetic: phonetic?.trim() || undefined,
            meanings: meaning ? [meaning.trim()] : [],
          };
        }).filter((w) => w.spelling);
        setUploadDraft({ ...uploadDraft(), id: uploadDraft().id || baseId, name: uploadDraft().name || baseId, wordsJson: JSON.stringify(words, null, 2) });
      } else {
        setUploadDraft({ ...uploadDraft(), id: uploadDraft().id || baseId, name: uploadDraft().name || baseId, wordsJson: txt });
      }
      setUploadOpen(true);
    };
    reader.readAsText(file);
  }

  async function submitUpload() {
    const d = uploadDraft();
    if (!d.id.trim() || !d.name.trim()) { uiStore.toast.error('ID 与名称必填'); return; }
    let words: Array<{ spelling: string; phonetic?: string; meanings?: string[]; examples?: string[] }>;
    try {
      const parsed = JSON.parse(d.wordsJson);
      if (!Array.isArray(parsed)) throw new Error('words 应为数组');
      words = parsed;
    } catch (e) {
      uiStore.toast.error('JSON 解析失败', e instanceof Error ? e.message : '');
      return;
    }
    setUploadBusy(true);
    try {
      const res = await adminApi.wbCenterUpload({
        id: d.id.trim(), name: d.name.trim(),
        description: d.description.trim() || undefined, words,
      });
      uiStore.toast.success(`已上传「${res.wordbook.name}」(${res.wordsImported} 词)`);
      setUploadOpen(false);
      setUploadDraft({ id: '', name: '', description: '', wordsJson: '[]' });
      await loadRemote();
      props.onImported();
    } catch (err: unknown) {
      uiStore.toast.error('上传失败', err instanceof Error ? err.message : '');
    } finally {
      setUploadBusy(false);
    }
  }

  function openTagsEditor(item: BrowseItem) {
    if (!item.localWordbookId) { uiStore.toast.error('该词书尚未导入,无法编辑本地标签'); return; }
    setTagsTarget(item);
    // 用本地标签预填(非远端 meta.tags),避免 replace 保存把远端标签污染进本地表
    setTagsDraft([...(item.localTags ?? [])]);
    setTagInput('');
  }
  async function saveTags() {
    const t = tagsTarget();
    if (!t || !t.localWordbookId) return;
    setTagsBusy(true);
    try {
      const res = await adminApi.wbCenterPatchTags(t.localWordbookId, { replace: tagsDraft() });
      uiStore.toast.success(`已更新标签(${res.tags.length} 个)`);
      setTagsTarget(null);
      await loadRemote();
    } catch (err: unknown) {
      uiStore.toast.error('标签保存失败', err instanceof Error ? err.message : '');
    } finally {
      setTagsBusy(false);
    }
  }

  function downloadTemplate() {
    const blob = new Blob([CSV_TEMPLATE], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'wordbook-template.csv';
    a.click();
    URL.revokeObjectURL(url);
  }

  let fileInput: HTMLInputElement | undefined;

  return (
    <div class="grid cols-12" style="margin-top:16px">
      {/* 远程目录浏览 */}
      <div class="panel" style="grid-column: span 7">
        <div class="panel-header">
          <h3>远程词库目录</h3>
          <span class="text-mono text-small text-tertiary">从配置的词书中心拉取 · 可导入为系统词库</span>
          <div class="aside">
            <Show when={configured()}>
              <button class="btn btn-secondary btn-sm" disabled={checking()} onClick={checkUpdates}>
                <Show when={checking()}><Spinner size="sm" /></Show>检查更新
              </button>
            </Show>
          </div>
        </div>
        <div class="panel-body">
          {/* 更新提示 */}
          <Show when={updates().length > 0}>
            <div class="row spread wrap" style="padding:10px 12px;border-radius:var(--radius-md);background:var(--accent-light);margin-bottom:12px;gap:8px">
              <span class="text-small" style="color:var(--accent)">{updates().length} 本词书有更新：{updates().map((u) => u.name).join('、')}</span>
              <div class="row gap-1 wrap">
                <For each={updates()}>
                  {(u) => (
                    <button class="btn btn-secondary btn-sm" disabled={mutating()} onClick={() => handleSync(u.remoteId, u.name)}>
                      <Show when={syncing() === u.remoteId}><Spinner size="sm" /></Show>同步「{u.name}」
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          <Show
            when={configured()}
            fallback={
              <Show when={!remoteLoading()}>
                <Empty title="尚未配置词书中心 URL" description="请前往「系统设置」配置全局词书中心地址后再使用远程导入" />
              </Show>
            }
          >
            <Show
              when={!remoteLoading()}
              fallback={<div class="row" style="justify-content:center;padding:32px 0"><Spinner size="lg" /></div>}
            >
              <Show
                when={remoteItems().length > 0}
                fallback={<Empty title="远程目录为空" description="远程源中没有可用的词书" />}
              >
                <div class="remote-grid">
                  <For each={remoteItems()}>
                    {(item) => (
                      <div class="remote-card">
                        <div class="row spread" style="align-items:flex-start;gap:6px">
                          <span class="name">{item.name}</span>
                          <Show when={item.imported}>
                            <span class={`chip ${item.hasUpdate ? 'chip-warning' : 'chip-success'}`} style="font-size:10px">
                              {item.hasUpdate ? '有更新' : '已导入'}
                            </span>
                          </Show>
                        </div>
                        <Show when={item.description}><span class="desc">{item.description}</span></Show>
                        <div class="meta">
                          <span>{fmtNum(item.wordCount)} 词</span>
                          <Show when={item.version}><span>v{item.version}</span></Show>
                          <Show when={item.author}><span>{item.author}</span></Show>
                        </div>
                        <Show when={item.imported && (item.localTags?.length ?? 0) > 0}>
                          <div class="row gap-1 wrap" style="margin-top:2px">
                            <For each={item.localTags}>
                              {(tag) => <span class="chip chip-outline" style="font-size:10px">{tag}</span>}
                            </For>
                          </div>
                        </Show>
                        <div class="row gap-1 wrap" style="margin-top:2px">
                          <button class="btn btn-ghost btn-sm" onClick={() => handlePreview(item.id)}>预览</button>
                          <Show when={!item.imported}>
                            <button class="btn btn-primary btn-sm" disabled={mutating()} onClick={() => setImportTarget(item)}>
                              <Show when={importing() === item.id}><Spinner size="sm" /></Show>导入
                            </button>
                          </Show>
                          <Show when={item.imported && item.hasUpdate}>
                            <button class="btn btn-secondary btn-sm" disabled={mutating()} onClick={() => handleSync(item.id, item.name)}>
                              <Show when={syncing() === item.id}><Spinner size="sm" /></Show>同步更新
                            </button>
                          </Show>
                          <Show when={item.imported && item.localWordbookId}>
                            <button class="btn btn-ghost btn-sm" onClick={() => openTagsEditor(item)}>编辑标签</button>
                          </Show>
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </Show>
        </div>
      </div>

      {/* 文件导入 */}
      <div class="panel" style="grid-column: span 5">
        <div class="panel-header">
          <h3>导入新词库</h3>
          <span class="text-mono text-small text-tertiary">CSV / JSON</span>
        </div>
        <div class="panel-body">
          <div
            class={`import-zone ${dragOver() ? 'is-drag' : ''}`}
            role="button"
            tabindex="0"
            onClick={() => fileInput?.click()}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); fileInput?.click(); } }}
            onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
            onDragLeave={() => setDragOver(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragOver(false);
              const f = e.dataTransfer?.files?.[0];
              if (f) handleFile(f);
            }}
          >
            <div class="ic">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/></svg>
            </div>
            <strong>拖拽文件到此处</strong>
            <span class="text-small text-secondary mt-2" style="display:block">或 <span class="text-accent">点击浏览</span> · 支持 CSV / JSON</span>
            <input
              ref={fileInput}
              type="file"
              accept=".json,.csv,application/json,text/csv"
              class="hidden"
              onChange={(e) => { const f = e.currentTarget.files?.[0]; if (f) handleFile(f); e.currentTarget.value = ''; }}
            />
          </div>
          <div class="divider" />
          <h4 style="font:600 12px/1.3 var(--font-display);margin-bottom:8px;color:var(--content-secondary);letter-spacing:0.02em;text-transform:uppercase">CSV 模板</h4>
          <div class="code-block">{CSV_TEMPLATE}</div>
          <div class="row gap-2 mt-3">
            <button class="btn btn-secondary btn-sm" onClick={downloadTemplate}>下载完整模板</button>
            <button class="btn btn-ghost btn-sm" onClick={() => setUploadOpen(true)}>手动粘贴 JSON</button>
          </div>
        </div>
      </div>

      {/* 导入确认 */}
      <ConfirmDialog
        open={!!importTarget()}
        title="确认导入词库"
        message={
          <Show when={importTarget()}>
            {(item) => (
              <>
                将把「<span class="font-medium text-content">{item().name}</span>」导入为系统词书(共 <span class="tabular-nums font-mono">{fmtNum(item().wordCount)}</span> 词)。
                <br />导入后系统词库将新增 / 合并对应单词,操作不可一键回滚。
              </>
            )}
          </Show>
        }
        confirmText="确认导入"
        variant="warning"
        onConfirm={() => { const it = importTarget(); setImportTarget(null); if (it) void handleImport(it); }}
        onCancel={() => setImportTarget(null)}
      />

      {/* 上传 modal */}
      <Modal open={uploadOpen()} onClose={() => setUploadOpen(false)} title="上传本地词库" size="lg">
        <div class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <Input
              label="词书 ID"
              placeholder="如 cet4-core"
              value={uploadDraft().id}
              onInput={(e) => setUploadDraft({ ...uploadDraft(), id: e.currentTarget.value })}
            />
            <Input
              label="词书名称"
              placeholder="如 CET-4 核心词汇"
              value={uploadDraft().name}
              onInput={(e) => setUploadDraft({ ...uploadDraft(), name: e.currentTarget.value })}
            />
          </div>
          <Input
            label="简介(可选)"
            placeholder="一句话说明用途"
            value={uploadDraft().description}
            onInput={(e) => setUploadDraft({ ...uploadDraft(), description: e.currentTarget.value })}
          />
          <div>
            <div class="flex items-baseline justify-between mb-1">
              <p class="text-sm font-medium text-content-secondary">Words JSON 数组</p>
              <label class="text-[11.5px] text-accent cursor-pointer hover:underline">
                选择 .json/.csv 文件
                <input
                  type="file"
                  accept=".json,.csv,application/json,text/csv"
                  class="hidden"
                  onChange={(e) => { const f = e.currentTarget.files?.[0]; if (f) handleFile(f); e.currentTarget.value = ''; }}
                />
              </label>
            </div>
            <textarea
              rows={10}
              value={uploadDraft().wordsJson}
              onInput={(e) => setUploadDraft({ ...uploadDraft(), wordsJson: e.currentTarget.value })}
              placeholder={'[{"spelling": "apple", "phonetic": "/ˈæpəl/", "meanings": ["苹果"]}]'}
              class="w-full px-3 py-2 rounded-md border border-border-hairline bg-surface text-[12px] font-mono focus:outline-none focus:border-accent"
            />
            <p class="text-[10.5px] text-content-tertiary mt-1">CSV 格式：<code class="font-mono">spelling,phonetic,meaning</code>(首行表头),客户端会自动转 JSON</p>
          </div>
          <div class="flex justify-end gap-2 pt-1">
            <button class="btn btn-ghost" onClick={() => setUploadOpen(false)}>取消</button>
            <button class="btn btn-primary" disabled={uploadBusy()} onClick={submitUpload}>
              <Show when={uploadBusy()}><Spinner size="sm" /></Show>上传
            </button>
          </div>
        </div>
      </Modal>

      {/* 预览 modal */}
      <Modal open={preview() !== null} onClose={() => setPreview(null)} title={preview()?.name ?? '词书预览'} size="lg">
        <Show when={preview()}>
          {(p) => (
            <div class="space-y-4 text-[13px]">
              <Show when={p().description}><p class="text-content-secondary leading-relaxed">{p().description}</p></Show>
              <div class="flex gap-4 text-content-secondary text-[12.5px]">
                <span>词条 <span class="font-mono text-content">{fmtNum(p().wordCount)}</span></span>
                <Show when={p().version}><span>版本 <span class="font-mono text-content">v{p().version}</span></span></Show>
                <Show when={p().author}><span>作者 <span class="text-content">{p().author}</span></span></Show>
              </div>
              <div>
                <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1.5">词条预览 · 显示前 {p().words.data.length} / {fmtNum(p().words.total)} 词</p>
                <ul class="space-y-2">
                  <For each={p().words.data}>
                    {(word) => (
                      <li class="px-3 py-2 rounded-md bg-surface-secondary">
                        <div class="flex items-center gap-2">
                          <span class="font-medium text-content">{word.spelling}</span>
                          <Show when={word.phonetic}><span class="font-mono text-[11.5px] text-content-tertiary">{word.phonetic}</span></Show>
                        </div>
                        <Show when={word.meanings.length > 0}>
                          <p class="text-content-secondary mt-1 text-[12.5px] leading-snug">{word.meanings.join('; ')}</p>
                        </Show>
                      </li>
                    )}
                  </For>
                </ul>
              </div>
            </div>
          )}
        </Show>
      </Modal>

      {/* 标签编辑 modal */}
      <Modal open={!!tagsTarget()} onClose={() => setTagsTarget(null)} title="编辑词书标签" size="md">
        <Show when={tagsTarget()}>
          {(t) => (
            <div class="space-y-3">
              <p class="text-sm text-content-secondary">
                正在编辑「<span class="font-medium text-content">{t().name}</span>」的本地标签。
                <br /><span class="text-[11.5px] text-content-tertiary">这只影响本地 wordbook_local_tags 表,远程词书原 tags 不受影响。</span>
              </p>
              <div class="flex flex-wrap gap-1.5 min-h-[32px] p-2 rounded-md border border-border-hairline bg-surface-secondary">
                <Show when={tagsDraft().length > 0} fallback={<span class="text-[11.5px] text-content-tertiary">暂无标签</span>}>
                  <For each={tagsDraft()}>
                    {(tag) => (
                      <span class="inline-flex items-center gap-1 px-2 py-0.5 bg-accent text-accent-content rounded-full text-xs">
                        {tag}
                        <button type="button" class="hover:opacity-70" onClick={() => setTagsDraft(tagsDraft().filter((x) => x !== tag))} aria-label={`移除标签 ${tag}`}>×</button>
                      </span>
                    )}
                  </For>
                </Show>
              </div>
              <div class="flex gap-2">
                <Input
                  placeholder="输入新标签按 Enter 添加"
                  value={tagInput()}
                  onInput={(e) => setTagInput(e.currentTarget.value)}
                  onKeyDown={(e: KeyboardEvent) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      const v = tagInput().trim();
                      if (v && !tagsDraft().includes(v)) setTagsDraft([...tagsDraft(), v]);
                      setTagInput('');
                    }
                  }}
                />
                <button class="btn btn-secondary btn-sm" onClick={() => {
                  const v = tagInput().trim();
                  if (v && !tagsDraft().includes(v)) setTagsDraft([...tagsDraft(), v]);
                  setTagInput('');
                }}>添加</button>
              </div>
              <div class="flex justify-end gap-2 pt-1">
                <button class="btn btn-ghost" onClick={() => setTagsTarget(null)}>取消</button>
                <button class="btn btn-primary" disabled={tagsBusy()} onClick={saveTags}>
                  <Show when={tagsBusy()}><Spinner size="sm" /></Show>保存
                </button>
              </div>
            </div>
          )}
        </Show>
      </Modal>
    </div>
  );
}
