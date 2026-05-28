import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
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

  return (
    <div class="space-y-6">
      <div class="flex items-center justify-between flex-wrap gap-2 pb-2 border-b border-border-hairline">
        <h1 class="text-title text-content">词书中心</h1>
        <Show when={configured()}>
          <Button size="sm" variant="ghost" onClick={checkUpdates} loading={checkingUpdates()}>
            检查更新
          </Button>
        </Show>
      </div>

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

      {/* Preview modal — 单一 open 控制；onClose 同步清 preview，避免下次打开闪旧数据 */}
      <Modal
        open={showPreview() && preview() !== null}
        onClose={closePreview}
        title={preview()?.name ?? ''}
        size="lg"
      >
        <Show when={preview()}>
          {(p) => (
            <div class="space-y-4 mt-2">
              <Show when={p().description}>
                <p class="text-sm text-content-secondary">{p().description}</p>
              </Show>
              <div class="flex gap-3 text-xs text-content-tertiary">
                <span>{p().wordCount} 词</span>
                <Show when={p().version}><span>v{p().version}</span></Show>
                <Show when={p().author}><span>作者: {p().author}</span></Show>
              </div>
              <div class="space-y-2 max-h-[400px] overflow-y-auto">
                <For each={p().words.data}>
                  {(word) => (
                    <div class="px-3 py-2 rounded-lg bg-surface-secondary text-sm">
                      <div class="flex items-center gap-2">
                        <span class="font-medium text-content">{word.spelling}</span>
                        <Show when={word.phonetic}>
                          <span class="text-content-tertiary">{word.phonetic}</span>
                        </Show>
                      </div>
                      <Show when={word.meanings.length > 0}>
                        <p class="text-content-secondary mt-1">{word.meanings.join('; ')}</p>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
              <Show when={p().words.totalPages > 1}>
                <p class="text-xs text-content-tertiary text-center">
                  显示前 {p().words.data.length} / {p().words.total} 词
                </p>
              </Show>
            </div>
          )}
        </Show>
      </Modal>
    </div>
  );
}
