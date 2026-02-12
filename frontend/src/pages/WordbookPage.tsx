import { createSignal, Show, For, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { Input } from '@/components/ui/Input';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { wordbooksApi } from '@/api/wordbooks';
import { studyConfigApi } from '@/api/studyConfig';
import type { Wordbook } from '@/types/wordbook';

export default function WordbookPage() {
  const navigate = useNavigate();
  const [systemBooks, setSystemBooks] = createSignal<Wordbook[]>([]);
  const [userBooks, setUserBooks] = createSignal<Wordbook[]>([]);
  const [selectedIds, setSelectedIds] = createSignal<string[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [showCreate, setShowCreate] = createSignal(false);
  const [saving, setSaving] = createSignal(false);

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
    if (saving()) return; // 请求进行中禁用点击
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
          <Badge variant={props.book.type === 'system' ? 'info' : 'accent'} size="sm">
            {props.book.type === 'system' ? '系统' : '自定义'}
          </Badge>
        </div>
        <div class="flex items-center gap-3 mt-3 text-xs text-content-tertiary">
          <span>{props.book.wordCount} 个单词</span>
          <Show when={isSelected()}>
            <Badge variant="success" size="sm">已选</Badge>
          </Show>
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
