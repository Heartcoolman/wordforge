import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { Input } from '@/components/ui/Input';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { wordbookCenterApi } from '@/api/wordbookCenter';
import type { BrowseItem, WordbookPreview, UpdateInfo } from '@/types/wordbookCenter';

export default function WordbookCenterPage() {
  const [items, setItems] = createSignal<BrowseItem[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [sourceUrl, setSourceUrl] = createSignal<string | null>(null);
  const [showSettings, setShowSettings] = createSignal(false);
  const [showImportUrl, setShowImportUrl] = createSignal(false);
  const [updates, setUpdates] = createSignal<UpdateInfo[]>([]);
  const [search, setSearch] = createSignal('');
  const [selectedTag, setSelectedTag] = createSignal<string | null>(null);
  const [preview, setPreview] = createSignal<WordbookPreview | null>(null);
  const [showPreview, setShowPreview] = createSignal(false);
  const [importing, setImporting] = createSignal<string | null>(null);
  const [syncing, setSyncing] = createSignal<string | null>(null);
  const [checkingUpdates, setCheckingUpdates] = createSignal(false);

  async function loadSettings() {
    try {
      const s = await wordbookCenterApi.getSettings();
      setSourceUrl(s.wordbookCenterUrl);
    } catch {
      // ignore
    }
  }

  async function loadItems() {
    setLoading(true);
    try {
      const data = await wordbookCenterApi.browse();
      setItems(data);
    } catch {
      // No URL configured or fetch failed → show empty
      setItems([]);
    } finally {
      setLoading(false);
    }
  }

  async function checkUpdates() {
    setCheckingUpdates(true);
    try {
      const data = await wordbookCenterApi.getUpdates();
      setUpdates(data);
      if (data.length === 0) uiStore.toast.success('所有词书均为最新');
    } catch (err: unknown) {
      uiStore.toast.error('检查更新失败', err instanceof Error ? err.message : '');
    } finally {
      setCheckingUpdates(false);
    }
  }

  onMount(async () => {
    await loadSettings();
    await loadItems();
  });

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

  async function handleImport(id: string) {
    setImporting(id);
    try {
      const res = await wordbookCenterApi.import(id);
      uiStore.toast.success(`已导入「${res.wordbook.name}」（${res.wordsImported} 词）`);
      await loadItems();
    } catch (err: unknown) {
      uiStore.toast.error('导入失败', err instanceof Error ? err.message : '');
    } finally {
      setImporting(null);
    }
  }

  async function handleSync(id: string) {
    setSyncing(id);
    try {
      const res = await wordbookCenterApi.sync(id);
      uiStore.toast.success(`同步完成：新增 ${res.wordsAdded}，更新 ${res.wordsUpdated}，移除 ${res.wordsRemoved}`);
      setUpdates((prev) => prev.filter((u) => u.remoteId !== id));
      await loadItems();
    } catch (err: unknown) {
      uiStore.toast.error('同步失败', err instanceof Error ? err.message : '');
    } finally {
      setSyncing(null);
    }
  }

  async function handlePreview(id: string) {
    try {
      const data = await wordbookCenterApi.preview(id, { perPage: 20 });
      setPreview(data);
      setShowPreview(true);
    } catch (err: unknown) {
      uiStore.toast.error('预览失败', err instanceof Error ? err.message : '');
    }
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <h1 class="text-2xl font-bold text-content">词书中心</h1>
        <div class="flex gap-2">
          <Button size="sm" variant="ghost" onClick={() => setShowSettings(true)}>
            源设置
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setShowImportUrl(true)}>
            自定义导入
          </Button>
          <Button size="sm" variant="ghost" onClick={checkUpdates} loading={checkingUpdates()}>
            检查更新
          </Button>
        </div>
      </div>

      <Show when={!sourceUrl() && !loading()}>
        <Card variant="outlined" padding="lg">
          <div class="text-center space-y-3">
            <p class="text-content-secondary">尚未设置词书源地址</p>
            <p class="text-sm text-content-tertiary">设置个人远程词书链接，浏览和导入在线词书</p>
            <Button onClick={() => setShowSettings(true)}>设置词书源</Button>
          </div>
        </Card>
      </Show>

      {/* Updates banner */}
      <Show when={updates().length > 0}>
        <Card variant="outlined" class="border-accent/50 bg-accent-light/30">
          <div class="flex items-center justify-between">
            <div>
              <p class="font-medium text-content">{updates().length} 本词书有更新</p>
              <p class="text-sm text-content-secondary">
                {updates().map((u) => u.name).join('、')}
              </p>
            </div>
          </div>
          <div class="flex flex-wrap gap-2 mt-3">
            <For each={updates()}>
              {(u) => (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => handleSync(u.remoteId)}
                  loading={syncing() === u.remoteId}
                >
                  同步「{u.name}」
                </Button>
              )}
            </For>
          </div>
        </Card>
      </Show>

      <Show when={sourceUrl() || loading()}>
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
                <div class="flex flex-wrap gap-1.5">
                  <button
                    class={`px-2 py-0.5 rounded text-xs transition-colors ${!selectedTag() ? 'bg-accent text-white' : 'bg-surface-tertiary text-content-secondary hover:bg-surface-secondary'}`}
                    onClick={() => setSelectedTag(null)}
                  >
                    全部
                  </button>
                  <For each={allTags()}>
                    {(tag) => (
                      <button
                        class={`px-2 py-0.5 rounded text-xs transition-colors ${selectedTag() === tag ? 'bg-accent text-white' : 'bg-surface-tertiary text-content-secondary hover:bg-surface-secondary'}`}
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
            <div class="grid sm:grid-cols-2 lg:grid-cols-3 gap-3 mt-4">
              <For each={filteredItems()}>
                {(item) => (
                  <Card
                    variant="outlined"
                    hover
                    padding="md"
                    onClick={() => handlePreview(item.id)}
                    class="cursor-pointer"
                  >
                    <div class="flex items-start justify-between gap-2">
                      <div class="min-w-0">
                        <h3 class="font-semibold text-content truncate">{item.name}</h3>
                        <Show when={item.description}>
                          <p class="text-sm text-content-secondary mt-1 line-clamp-2">{item.description}</p>
                        </Show>
                      </div>
                      <Show when={item.imported}>
                        <Badge variant={item.hasUpdate ? 'warning' : 'success'} size="sm">
                          {item.hasUpdate ? '有更新' : '已导入'}
                        </Badge>
                      </Show>
                    </div>
                    <div class="flex items-center gap-3 mt-3 text-xs text-content-tertiary">
                      <span>{item.wordCount} 词</span>
                      <Show when={item.version}><span>v{item.version}</span></Show>
                      <Show when={item.author}><span>{item.author}</span></Show>
                    </div>
                    <Show when={item.tags.length > 0}>
                      <div class="flex flex-wrap gap-1 mt-2">
                        <For each={item.tags.slice(0, 3)}>
                          {(tag) => <Badge size="sm">{tag}</Badge>}
                        </For>
                      </div>
                    </Show>
                    <div class="mt-3" onClick={(e: MouseEvent) => e.stopPropagation()}>
                      <Show when={!item.imported}>
                        <Button
                          size="sm"
                          onClick={() => handleImport(item.id)}
                          loading={importing() === item.id}
                        >
                          导入
                        </Button>
                      </Show>
                      <Show when={item.imported && item.hasUpdate}>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => handleSync(item.id)}
                          loading={syncing() === item.id}
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

      {/* Settings modal */}
      <SettingsModal
        open={showSettings()}
        url={sourceUrl()}
        onClose={() => setShowSettings(false)}
        onSaved={async (url) => {
          setSourceUrl(url);
          setShowSettings(false);
          await loadItems();
        }}
      />

      {/* Import URL modal */}
      <ImportUrlModal
        open={showImportUrl()}
        onClose={() => setShowImportUrl(false)}
        onImported={() => {
          setShowImportUrl(false);
          loadItems();
        }}
      />

      {/* Preview modal */}
      <Show when={preview()}>
        {(p) => (
          <Modal open={showPreview()} onClose={() => setShowPreview(false)} title={p().name} size="lg">
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
          </Modal>
        )}
      </Show>
    </div>
  );
}

function SettingsModal(props: {
  open: boolean;
  url: string | null;
  onClose: () => void;
  onSaved: (url: string | null) => void;
}) {
  const [url, setUrl] = createSignal(props.url || '');
  const [saving, setSaving] = createSignal(false);

  // Sync initial url when opening
  const handleSave = async () => {
    setSaving(true);
    try {
      const val = url().trim() || null;
      await wordbookCenterApi.updateSettings({ wordbookCenterUrl: val });
      uiStore.toast.success('词书源已更新');
      props.onSaved(val);
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : '');
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal open={props.open} onClose={props.onClose} title="词书源设置">
      <div class="space-y-3 mt-2">
        <Input
          label="远程词书源 URL"
          value={url()}
          onInput={(e) => setUrl(e.currentTarget.value)}
          placeholder="https://example.com/wordbooks"
        />
        <p class="text-xs text-content-tertiary">
          输入兼容格式的远程词书源地址，需包含 index.json 和 wordbooks/ 目录
        </p>
        <div class="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={props.onClose}>取消</Button>
          <Button onClick={handleSave} loading={saving()}>保存</Button>
        </div>
      </div>
    </Modal>
  );
}

function ImportUrlModal(props: {
  open: boolean;
  onClose: () => void;
  onImported: () => void;
}) {
  const [url, setUrl] = createSignal('');
  const [importing, setImporting] = createSignal(false);

  const handleImport = async () => {
    if (!url().trim()) { uiStore.toast.warning('请输入 URL'); return; }
    setImporting(true);
    try {
      const res = await wordbookCenterApi.importUrl(url().trim());
      uiStore.toast.success(`已导入「${res.wordbook.name}」（${res.wordsImported} 词）`);
      setUrl('');
      props.onImported();
    } catch (err: unknown) {
      uiStore.toast.error('导入失败', err instanceof Error ? err.message : '');
    } finally {
      setImporting(false);
    }
  };

  return (
    <Modal open={props.open} onClose={props.onClose} title="自定义 URL 导入">
      <div class="space-y-3 mt-2">
        <Input
          label="词书 JSON URL"
          value={url()}
          onInput={(e) => setUrl(e.currentTarget.value)}
          placeholder="https://example.com/wordbooks/cet4.json"
        />
        <p class="text-xs text-content-tertiary">
          输入词书 JSON 文件的直接链接，格式需包含 words 数组
        </p>
        <div class="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={props.onClose}>取消</Button>
          <Button onClick={handleImport} loading={importing()}>导入</Button>
        </div>
      </div>
    </Modal>
  );
}
