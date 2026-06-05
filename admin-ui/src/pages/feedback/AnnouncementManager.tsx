import { createSignal, createResource, For, Show } from 'solid-js';
import { Modal } from '@/components/ui/Modal';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Switch } from '@/components/ui/Switch';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import type { AnnouncementKind, FeedbackAnnouncement } from '@/api/admin';

/**
 * 公告 / FAQ 管理弹窗 —— 对齐设计稿"新建公告 / FAQ"入口。
 * 列出现有公告(可发布切换 / 编辑 / 删除)并提供新建 / 编辑表单。
 */
export function AnnouncementManager(props: { open: boolean; onClose: () => void }) {
  const [items, { refetch }] = createResource(
    () => (props.open ? true : undefined),
    () => adminApi.feedbackAnnouncements().then((r) => r.data),
  );

  // 表单(editing=null 表示新建)
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [title, setTitle] = createSignal('');
  const [body, setBody] = createSignal('');
  const [kind, setKind] = createSignal<AnnouncementKind>('announcement');
  const [published, setPublished] = createSignal(false);
  const [saving, setSaving] = createSignal(false);

  function resetForm() {
    setEditingId(null);
    setTitle('');
    setBody('');
    setKind('announcement');
    setPublished(false);
  }

  function startEdit(a: FeedbackAnnouncement) {
    setEditingId(a.id);
    setTitle(a.title);
    setBody(a.body);
    setKind(a.kind);
    setPublished(a.published);
  }

  async function onSave() {
    const t = title().trim();
    const b = body().trim();
    if (!t || !b) {
      uiStore.toast.warning('请填写完整', '标题与正文均不能为空');
      return;
    }
    setSaving(true);
    try {
      const id = editingId();
      if (id != null) {
        await adminApi.updateFeedbackAnnouncement(id, { title: t, body: b, kind: kind(), published: published() });
        uiStore.toast.success('已更新');
      } else {
        await adminApi.createFeedbackAnnouncement({ title: t, body: b, kind: kind(), published: published() });
        uiStore.toast.success('已创建');
      }
      resetForm();
      refetch();
    } catch (e) {
      uiStore.toast.error('保存失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setSaving(false);
    }
  }

  async function onTogglePublish(a: FeedbackAnnouncement) {
    try {
      await adminApi.updateFeedbackAnnouncement(a.id, { published: !a.published });
      refetch();
    } catch (e) {
      uiStore.toast.error('操作失败', e instanceof Error ? e.message : '未知错误');
    }
  }

  async function onDelete(a: FeedbackAnnouncement) {
    if (!window.confirm(`确认删除「${a.title}」?`)) return;
    try {
      await adminApi.deleteFeedbackAnnouncement(a.id);
      if (editingId() === a.id) resetForm();
      uiStore.toast.success('已删除');
      refetch();
    } catch (e) {
      uiStore.toast.error('删除失败', e instanceof Error ? e.message : '未知错误');
    }
  }

  function onClose() {
    resetForm();
    props.onClose();
  }

  return (
    <Modal open={props.open} onClose={onClose} title="公告 / FAQ 管理" size="lg">
      <Show
        when={!items.loading}
        fallback={
          <div class="py-10 flex justify-center">
            <Spinner size="lg" />
          </div>
        }
      >
        <div class="fb-ann-list">
          <Show
            when={(items() ?? []).length > 0}
            fallback={<Empty title="暂无公告 / FAQ" description="使用下方表单创建第一条" />}
          >
            <For each={items()}>
              {(a) => (
                <div class="fb-ann-row">
                  <div class="main">
                    <div class="ttl">
                      {a.title}
                      <span class={`fb-ann-kind ${a.kind}`}>{a.kind === 'faq' ? 'FAQ' : '公告'}</span>
                      <Show when={!a.published}>
                        <span class="fb-ann-kind draft">草稿</span>
                      </Show>
                    </div>
                    <div class="bd">{a.body}</div>
                  </div>
                  <div class="row-acts">
                    <Button variant="ghost" size="sm" onClick={() => onTogglePublish(a)}>
                      {a.published ? '下架' : '发布'}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => startEdit(a)}>
                      编辑
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => onDelete(a)}>
                      删除
                    </Button>
                  </div>
                </div>
              )}
            </For>
          </Show>
        </div>
      </Show>

      <div class="fb-ann-form">
        <div class="text-[12px] font-semibold text-content-secondary">
          {editingId() != null ? '编辑条目' : '新建条目'}
        </div>
        <Input
          value={title()}
          onInput={(e) => setTitle(e.currentTarget.value)}
          placeholder="标题"
          aria-label="标题"
        />
        <textarea
          class="w-full px-3 py-2 rounded-md border border-border bg-surface text-[13px] resize-y focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/30 transition"
          rows={3}
          value={body()}
          onInput={(e) => setBody(e.currentTarget.value)}
          placeholder="正文(支持 Markdown)"
          aria-label="正文"
        />
        <div class="row">
          <div class="inline-flex rounded-md border border-border-hairline overflow-hidden">
            <button
              type="button"
              aria-pressed={kind() === 'announcement'}
              onClick={() => setKind('announcement')}
              class={`px-3 py-1.5 text-[12px] font-medium border-r border-border-hairline transition-colors ${
                kind() === 'announcement' ? 'bg-accent text-white' : 'bg-surface-secondary text-content-secondary hover:bg-surface-tertiary'
              }`}
            >
              公告
            </button>
            <button
              type="button"
              aria-pressed={kind() === 'faq'}
              onClick={() => setKind('faq')}
              class={`px-3 py-1.5 text-[12px] font-medium transition-colors ${
                kind() === 'faq' ? 'bg-accent text-white' : 'bg-surface-secondary text-content-secondary hover:bg-surface-tertiary'
              }`}
            >
              FAQ
            </button>
          </div>
          <Switch checked={published()} onChange={setPublished} label="立即发布" />
          <div class="ml-auto flex gap-2">
            <Show when={editingId() != null}>
              <Button variant="ghost" size="sm" onClick={resetForm}>
                取消编辑
              </Button>
            </Show>
            <Button variant="primary" size="sm" loading={saving()} onClick={onSave}>
              {editingId() != null ? '保存修改' : '创建'}
            </Button>
          </div>
        </div>
      </div>
    </Modal>
  );
}
