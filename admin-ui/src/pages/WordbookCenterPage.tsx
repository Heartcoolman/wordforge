import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { HeroCard } from '@/components/ui/HeroCard';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Drawer } from '@/components/ui/Drawer';
import { Modal } from '@/components/ui/Modal';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Input } from '@/components/ui/Input';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import type { BrowseItem, WordbookPreview, UpdateInfo } from '@/types/wordbookCenter';

export default function WordbookCenterPage() {
  const [items, setItems] = createSignal<BrowseItem[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [configured, setConfigured] = createSignal(false);
  const [updates, setUpdates] = createSignal<UpdateInfo[]>([]);
  const [search, setSearch] = createSignal('');
  const [selectedTag, setSelectedTag] = createSignal<string | null>(null);
  const [preview, setPreview] = createSignal<WordbookPreview | null>(null);
  const [showPreview, setShowPreview] = createSignal(false);
  const [importing, setImporting] = createSignal<string | null>(null);
  const [syncing, setSyncing] = createSignal<string | null>(null);
  const [checkingUpdates, setCheckingUpdates] = createSignal(false);
  const [importTarget, setImportTarget] = createSignal<BrowseItem | null>(null);
  // m022:本地词书上传 modal
  const [uploadOpen, setUploadOpen] = createSignal(false);
  const [uploadDraft, setUploadDraft] = createSignal({ id: '', name: '', description: '', wordsJson: '[]' });
  const [uploadBusy, setUploadBusy] = createSignal(false);
  // m022:本地词书标签编辑 modal(只对 imported 词书有效)
  const [tagsTarget, setTagsTarget] = createSignal<BrowseItem | null>(null);
  const [tagsDraft, setTagsDraft] = createSignal<string[]>([]);
  const [tagsBusy, setTagsBusy] = createSignal(false);
  const [tagInput, setTagInput] = createSignal('');

  async function loadItems() {
    setLoading(true);
    try {
      const settings = await adminApi.getSettings();
      if (!settings.wordbookCenterUrl) {
        setConfigured(false);
        setItems([]);
        return;
      }
      setConfigured(true);
      try {
        const data = await adminApi.wbCenterBrowse();
        setItems(data);
      } catch (err: unknown) {
        // 配置已存在但 fetch 远端失败 — 用户能看出端倪
        setItems([]);
        setUpdates([]);
        uiStore.toast.error('加载词书中心失败', err instanceof Error ? err.message : '');
      }
    } catch (err: unknown) {
      setItems([]);
      setConfigured(false);
      setUpdates([]);
      uiStore.toast.error('读取设置失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  async function checkUpdates() {
    setCheckingUpdates(true);
    try {
      const data = await adminApi.wbCenterUpdates();
      setUpdates(data);
      if (data.length === 0) uiStore.toast.success('所有词书均为最新');
    } catch (err: unknown) {
      uiStore.toast.error('检查更新失败', err instanceof Error ? err.message : '');
    } finally {
      setCheckingUpdates(false);
    }
  }

  onMount(loadItems);

  // 任一 import/sync 在飞 → 禁用所有 mutating 按钮，避免并发请求
  const mutating = () => importing() !== null || syncing() !== null;

  const filteredItems = () => {
    let list = items();
    const q = search().toLowerCase().trim();
    if (q) list = list.filter((i) => i.name.toLowerCase().includes(q) || i.description.toLowerCase().includes(q));
    const tag = selectedTag();
    if (tag) list = list.filter((i) => i.tags.includes(tag));
    return list;
  };

  const allTags = () => {
    const tags = new Set<string>();
    items().forEach((i) => i.tags.forEach((t) => tags.add(t)));
    return [...tags].sort();
  };

  async function handleImport(item: BrowseItem) {
    setImporting(item.id);
    try {
      const res = await adminApi.wbCenterImport(item.id);
      uiStore.toast.success(`已导入「${res.wordbook.name}」（${res.wordsImported} 词）`);
      await loadItems();
    } catch (err: unknown) {
      uiStore.toast.error(`导入「${item.name}」失败`, err instanceof Error ? err.message : '');
    } finally {
      setImporting(null);
    }
  }

  function confirmImport() {
    const item = importTarget();
    if (item) {
      setImportTarget(null);
      void handleImport(item);
    }
  }

  async function handleSync(id: string, name?: string) {
    setSyncing(id);
    try {
      const res = await adminApi.wbCenterSync(id);
      uiStore.toast.success(`同步完成：新增 ${res.wordsAdded}，更新 ${res.wordsUpdated}，移除 ${res.wordsRemoved}`);
      setUpdates((prev) => prev.filter((u) => u.remoteId !== id));
      await loadItems();
    } catch (err: unknown) {
      uiStore.toast.error(name ? `同步「${name}」失败` : '同步失败', err instanceof Error ? err.message : '');
    } finally {
      setSyncing(null);
    }
  }

  async function handlePreview(id: string) {
    try {
      const data = await adminApi.wbCenterPreview(id, { perPage: 20 });
      setPreview(data);
      setShowPreview(true);
    } catch (err: unknown) {
      uiStore.toast.error('预览失败', err instanceof Error ? err.message : '');
    }
  }

  function closePreview() {
    setShowPreview(false);
    setPreview(null);
  }

  // m022:上传本地词书 → POST /admin/wordbook-center/upload
  async function submitUpload() {
    const d = uploadDraft();
    if (!d.id.trim() || !d.name.trim()) {
      uiStore.toast.error('ID 与名称必填');
      return;
    }
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
        id: d.id.trim(),
        name: d.name.trim(),
        description: d.description.trim() || undefined,
        words,
      });
      uiStore.toast.success(`已上传「${res.wordbook.name}」(${res.wordsImported} 词)`);
      setUploadOpen(false);
      setUploadDraft({ id: '', name: '', description: '', wordsJson: '[]' });
      await loadItems();
    } catch (err: unknown) {
      uiStore.toast.error('上传失败', err instanceof Error ? err.message : '');
    } finally {
      setUploadBusy(false);
    }
  }

  // m022:文件选择 → 读 JSON / CSV(PapaParse 走客户端 → 转成 words 数组)
  function handleFile(file: File) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const txt = e.target?.result;
      if (typeof txt !== 'string') return;
      if (file.name.endsWith('.csv')) {
        // 简易 CSV: spelling,phonetic,meaning 三列
        const lines = txt.split(/\r?\n/).filter((l) => l.trim());
        const words = lines.slice(1).map((line) => {
          const [spelling, phonetic, meaning] = line.split(',');
          return {
            spelling: spelling?.trim() ?? '',
            phonetic: phonetic?.trim() || undefined,
            meanings: meaning ? [meaning.trim()] : [],
          };
        }).filter((w) => w.spelling);
        setUploadDraft({ ...uploadDraft(), wordsJson: JSON.stringify(words, null, 2) });
      } else {
        setUploadDraft({ ...uploadDraft(), wordsJson: txt });
      }
    };
    reader.readAsText(file);
  }

  // m022:打开标签编辑 modal
  function openTagsEditor(item: BrowseItem) {
    if (!item.localWordbookId) {
      uiStore.toast.error('该词书尚未导入,无法编辑本地标签');
      return;
    }
    setTagsTarget(item);
    setTagsDraft([...item.tags]);
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
      await loadItems();
    } catch (err: unknown) {
      uiStore.toast.error('标签保存失败', err instanceof Error ? err.message : '');
    } finally {
      setTagsBusy(false);
    }
  }

  return (
    <div class="space-y-6">
      <HeroCard
        eyebrow="词条管理"
        eyebrowVariant="accent"
        title="词书中心"
        desc="管理官方与用户词库。支持词条预览、标签编辑、CSV/JSON 导入。"
        cta={
          <div class="flex flex-wrap gap-2">
            <Button size="sm" onClick={() => setUploadOpen(true)}>
              上传本地词书
            </Button>
            <Show when={configured()}>
              <Button size="sm" variant="ghost" onClick={checkUpdates} loading={checkingUpdates()}>
                检查更新
              </Button>
            </Show>
          </div>
        }
      />

      <Show when={!configured() && !loading()}>
        <Card variant="outlined" padding="lg">
          <div class="text-center space-y-3">
            <p class="text-content-secondary">尚未配置词书中心 URL</p>
            <p class="text-sm text-content-tertiary">请前往「系统设置」页面配置全局词书中心地址</p>
          </div>
        </Card>
      </Show>

      {/* Updates banner — items-start 防紧贴 + 按钮组与文字间距充足 */}
      <Show when={updates().length > 0}>
        <Card variant="outlined" class="border-accent/50 bg-accent-light/30">
          <div class="flex flex-col gap-4">
            <div class="flex items-start justify-between gap-3">
              <div>
                <p class="font-medium text-content">{updates().length} 本词书有更新</p>
                <p class="text-sm text-content-secondary">
                  {updates().map((u) => u.name).join('、')}
                </p>
              </div>
            </div>
            <div class="flex flex-wrap gap-2">
              <For each={updates()}>
                {(u) => (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => handleSync(u.remoteId, u.name)}
                    loading={syncing() === u.remoteId}
                  >
                    同步「{u.name}」
                  </Button>
                )}
              </For>
            </div>
          </div>
        </Card>
      </Show>

      <Show when={configured() || loading()}>
        <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
          <Show when={items().length > 0} fallback={
            <Empty title="暂无词书" description="远程源中没有可用的词书" />
          }>
            {/* Search + tags */}
            <div class="space-y-3">
              <Input
                placeholder="搜索词书..."
                value={search()}
                onInput={(e) => setSearch(e.currentTarget.value)}
              />
              <Show when={allTags().length > 0}>
                <div class="flex flex-wrap gap-1.5" role="group" aria-label="标签筛选">
                  <button
                    type="button"
                    aria-pressed={!selectedTag()}
                    class={`focus-ring-soft px-2 py-0.5 rounded-full text-xs whitespace-nowrap transition-all duration-fast ${!selectedTag() ? 'bg-accent text-accent-content' : 'bg-surface-tertiary text-content-secondary hover:bg-surface-secondary'}`}
                    onClick={() => setSelectedTag(null)}
                  >
                    全部
                  </button>
                  <For each={allTags()}>
                    {(tag) => (
                      <button
                        type="button"
                        aria-pressed={selectedTag() === tag}
                        class={`focus-ring-soft px-2 py-0.5 rounded-full text-xs whitespace-nowrap transition-all duration-fast ${selectedTag() === tag ? 'bg-accent text-accent-content' : 'bg-surface-tertiary text-content-secondary hover:bg-surface-secondary'}`}
                        onClick={() => setSelectedTag(selectedTag() === tag ? null : tag)}
                      >
                        {tag}
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </div>

            {/* Grid */}
            <Show
              when={filteredItems().length > 0}
              fallback={
                <Card variant="outlined" padding="lg" class="mt-4">
                  <Empty
                    title="无匹配词书"
                    description={search().trim() ? `没有词书匹配「${search()}」，请尝试其他关键词或清除筛选` : '当前筛选条件下没有词书'}
                  />
                </Card>
              }
            >
            <div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-4 gap-3 mt-4">
              <For each={filteredItems()}>
                {(item) => (
                  <Card
                    variant="outlined"
                    hover
                    padding="md"
                    onClick={() => handlePreview(item.id)}
                    onKeyDown={(e: KeyboardEvent) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        handlePreview(item.id);
                      }
                    }}
                    role="button"
                    tabindex="0"
                    aria-label={`预览词书 ${item.name}`}
                    class="cursor-pointer focus-ring-soft"
                  >
                    <div class="flex items-start justify-between gap-2">
                      <div class="flex-1 min-w-0">
                        <h3 class="font-semibold text-content truncate">{item.name}</h3>
                        <Show when={item.description}>
                          <p class="text-sm text-content-secondary mt-0.5 line-clamp-2">{item.description}</p>
                        </Show>
                      </div>
                      <Show when={item.imported}>
                        <Badge variant={item.hasUpdate ? 'warning' : 'success'} size="sm">
                          {item.hasUpdate ? '有更新' : '已导入'}
                        </Badge>
                      </Show>
                    </div>
                    <div class="flex items-center gap-3 mt-2 text-xs text-content-tertiary tabular-nums min-w-0">
                      <span class="shrink-0">{item.wordCount} 词</span>
                      <Show when={item.version}><span class="shrink-0">v{item.version}</span></Show>
                      <Show when={item.author}><span class="truncate min-w-0" title={item.author}>{item.author}</span></Show>
                    </div>
                    <Show when={item.tags.length > 0}>
                      <div class="flex flex-wrap gap-1 mt-1.5">
                        <For each={item.tags.slice(0, 3)}>
                          {(tag) => <Badge size="sm">{tag}</Badge>}
                        </For>
                      </div>
                    </Show>
                    <div class="mt-2.5" onClick={(e: MouseEvent) => e.stopPropagation()}>
                      <Show when={!item.imported}>
                        <Button
                          size="sm"
                          onClick={() => setImportTarget(item)}
                          loading={importing() === item.id}
                          disabled={mutating()}
                        >
                          导入为系统词书
                        </Button>
                      </Show>
                      <Show when={item.imported && item.hasUpdate}>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => handleSync(item.id, item.name)}
                          loading={syncing() === item.id}
                          disabled={mutating()}
                        >
                          同步更新
                        </Button>
                      </Show>
                    </div>
                  </Card>
                )}
              </For>
            </div>
            </Show>
          </Show>
        </Show>
      </Show>

      {/* 导入确认弹窗 */}
      <Show when={importTarget()}>
        {(item) => (
          <ConfirmDialog
            open={true}
            title="确认导入词书"
            message={
              <>
                将把「<span class="font-medium text-content">{item().name}</span>」导入为系统词书（共 <span class="tabular-nums font-mono">{item().wordCount}</span> 词）。
                <br />导入后系统词库将新增/合并对应单词，操作不可一键回滚。
              </>
            }
            confirmText="确认导入"
            variant="warning"
            onConfirm={confirmImport}
            onCancel={() => setImportTarget(null)}
          />
        )}
      </Show>

      {/* Preview drawer — 替代原 Modal:右侧滑入,元信息更紧凑,底部 footer 直接操作 */}
      <Drawer
        open={showPreview() && preview() !== null}
        onClose={closePreview}
        title={preview()?.name ?? '词书预览'}
        width={460}
        headerActions={
          <Show when={preview()?.tags && preview()!.tags.length > 0}>
            <span class="text-[11.5px] text-content-tertiary tabular-nums" title={preview()!.tags.join(', ')}>
              {preview()!.tags.length} 标签
            </span>
          </Show>
        }
        footer={
          <Show when={preview()}>
            {(p) => {
              // 找到 grid 中对应 item 拿 imported / hasUpdate 状态
              const matched = () => items().find((it) => it.id === p().id);
              return (
                <div class="flex flex-wrap justify-end gap-2">
                  <Button variant="ghost" size="sm" onClick={closePreview}>关闭</Button>
                  <Show when={matched()?.imported && matched()?.localWordbookId}>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => {
                        const it = matched();
                        if (it) {
                          openTagsEditor(it);
                          closePreview();
                        }
                      }}
                    >
                      编辑标签
                    </Button>
                  </Show>
                  <Show when={matched() && !matched()!.imported}>
                    <Button
                      size="sm"
                      onClick={() => {
                        const it = matched();
                        if (it) setImportTarget(it);
                      }}
                      loading={importing() === p().id}
                      disabled={mutating()}
                    >
                      导入为系统词书
                    </Button>
                  </Show>
                  <Show when={matched()?.imported && matched()?.hasUpdate}>
                    <Button
                      size="sm"
                      onClick={() => handleSync(p().id, p().name)}
                      loading={syncing() === p().id}
                      disabled={mutating()}
                    >
                      同步更新
                    </Button>
                  </Show>
                </div>
              );
            }}
          </Show>
        }
      >
        <Show when={preview()}>
          {(p) => (
            <div class="space-y-4 text-[13px]">
              {/* metadata grid */}
              <dl class="grid grid-cols-[80px_1fr] gap-y-2 gap-x-3 text-content-secondary">
                <Show when={p().description}>
                  <dt>简介</dt>
                  <dd class="text-content leading-relaxed">{p().description}</dd>
                </Show>
                <dt>词条数</dt>
                <dd class="font-mono tabular-nums text-content">{p().wordCount}</dd>
                <Show when={p().version}>
                  <dt>版本</dt>
                  <dd class="font-mono text-content">v{p().version}</dd>
                </Show>
                <Show when={p().author}>
                  <dt>作者</dt>
                  <dd class="text-content">{p().author}</dd>
                </Show>
                <Show when={p().tags && p().tags.length > 0}>
                  <dt>标签</dt>
                  <dd class="flex flex-wrap gap-1">
                    <For each={p().tags}>
                      {(tag) => <Badge size="sm">{tag}</Badge>}
                    </For>
                  </dd>
                </Show>
              </dl>

              {/* preview 词条列表 */}
              <div>
                <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1.5">
                  词条预览 · 显示前 {p().words.data.length} / {p().words.total} 词
                </p>
                <ul class="space-y-2">
                  <For each={p().words.data}>
                    {(word) => (
                      <li class="px-3 py-2 rounded-md bg-surface-secondary">
                        <div class="flex items-center gap-2">
                          <span class="font-medium text-content">{word.spelling}</span>
                          <Show when={word.phonetic}>
                            <span class="font-mono text-[11.5px] text-content-tertiary">{word.phonetic}</span>
                          </Show>
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
      </Drawer>

      {/* m022:本地词书上传 Modal */}
      <Modal open={uploadOpen()} onClose={() => setUploadOpen(false)} title="上传本地词书" size="lg">
        <div class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <div>
              <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">词书 ID</p>
              <Input
                placeholder="如 cet4-core (英数 - 下划线)"
                value={uploadDraft().id}
                onInput={(e) => setUploadDraft({ ...uploadDraft(), id: e.currentTarget.value })}
              />
            </div>
            <div>
              <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">词书名称</p>
              <Input
                placeholder="如 CET-4 核心词汇"
                value={uploadDraft().name}
                onInput={(e) => setUploadDraft({ ...uploadDraft(), name: e.currentTarget.value })}
              />
            </div>
          </div>
          <div>
            <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">简介(可选)</p>
            <Input
              placeholder="一句话说明用途"
              value={uploadDraft().description}
              onInput={(e) => setUploadDraft({ ...uploadDraft(), description: e.currentTarget.value })}
            />
          </div>
          <div>
            <div class="flex items-baseline justify-between mb-1">
              <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary">
                Words JSON 数组
              </p>
              <label class="text-[11.5px] text-accent cursor-pointer hover:underline">
                选择 .json/.csv 文件
                <input
                  type="file"
                  accept=".json,.csv,application/json,text/csv"
                  class="hidden"
                  onChange={(e) => {
                    const f = e.currentTarget.files?.[0];
                    if (f) handleFile(f);
                  }}
                />
              </label>
            </div>
            <textarea
              rows={10}
              value={uploadDraft().wordsJson}
              onInput={(e) => setUploadDraft({ ...uploadDraft(), wordsJson: e.currentTarget.value })}
              placeholder={`[{"spelling": "apple", "phonetic": "/ˈæpəl/", "meanings": ["苹果"]}]`}
              class="w-full px-3 py-2 rounded-md border border-border bg-surface text-[12px] font-mono focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/30 transition"
            />
            <p class="text-[10.5px] text-content-tertiary mt-1">
              CSV 格式: <code class="font-mono">spelling,phonetic,meaning</code>(首行表头),客户端会自动转 JSON
            </p>
          </div>
          <div class="flex justify-end gap-2 pt-1">
            <Button size="sm" variant="ghost" onClick={() => setUploadOpen(false)}>取消</Button>
            <Button size="sm" loading={uploadBusy()} onClick={submitUpload}>上传</Button>
          </div>
        </div>
      </Modal>

      {/* m022:本地词书标签编辑 Modal */}
      <Modal open={!!tagsTarget()} onClose={() => setTagsTarget(null)} title="编辑词书标签" size="md">
        <Show when={tagsTarget()}>
          {(t) => (
            <div class="space-y-3">
              <p class="text-sm text-content-secondary">
                正在编辑「<span class="font-medium text-content">{t().name}</span>」的本地标签。
                <br />
                <span class="text-[11.5px] text-content-tertiary">
                  这只影响本地 wordbook_local_tags 表,远程词书原 tags 不受影响。
                </span>
              </p>
              {/* 当前标签 chips */}
              <div class="flex flex-wrap gap-1.5 min-h-[32px] p-2 rounded-md border border-border-hairline bg-surface-secondary">
                <Show when={tagsDraft().length > 0} fallback={<span class="text-[11.5px] text-content-tertiary">暂无标签</span>}>
                  <For each={tagsDraft()}>
                    {(tag) => (
                      <span class="inline-flex items-center gap-1 px-2 py-0.5 bg-accent text-accent-content rounded-full text-xs">
                        {tag}
                        <button
                          type="button"
                          class="hover:text-accent-content/70"
                          onClick={() => setTagsDraft(tagsDraft().filter((x) => x !== tag))}
                          aria-label={`移除标签 ${tag}`}
                        >×</button>
                      </span>
                    )}
                  </For>
                </Show>
              </div>
              {/* 新增标签 */}
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
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    const v = tagInput().trim();
                    if (v && !tagsDraft().includes(v)) setTagsDraft([...tagsDraft(), v]);
                    setTagInput('');
                  }}
                >
                  添加
                </Button>
              </div>
              <div class="flex justify-end gap-2 pt-1">
                <Button size="sm" variant="ghost" onClick={() => setTagsTarget(null)}>取消</Button>
                <Button size="sm" loading={tagsBusy()} onClick={saveTags}>保存</Button>
              </div>
            </div>
          )}
        </Show>
      </Modal>
    </div>
  );
}
