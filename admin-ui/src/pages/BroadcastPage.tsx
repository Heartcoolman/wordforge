import { createSignal, Show, For } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Input, TextArea } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Tabs } from '@/components/ui/Tabs';
import { Badge } from '@/components/ui/Badge';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { HeroCard } from '@/components/ui/HeroCard';
import { SectionTitle } from '@/components/ui/SectionTitle';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';

/**
 * 系统广播页。原 SettingsPage 内嵌的广播 + 更新通知拆出独立页。
 *
 * 三种广播类型（按设计图 broadcast.html）：
 *   - 系统消息（最常用）：通用通知，需标题 + 内容
 *   - 更新通知：触发用户刷新，可选附加文字
 *   - 维护通告：仅提示，不强制行为
 *
 * 后端 endpoint：
 *   - POST /api/admin/broadcast            → 系统消息
 *   - POST /api/admin/broadcast-update     → 更新通知
 *   - （维护通告本质也走 /broadcast，但模板预填 + 不同 tone）
 */
type Tab = 'system' | 'update' | 'maintenance';

interface SendHistoryEntry {
  ts: string;
  tab: Tab;
  title?: string;
  message?: string;
  sentCount?: number;
  ok: boolean;
}

const tabMeta: Record<Tab, { label: string; chip: string; chipVariant: 'accent' | 'info' | 'warning'; hint: string }> = {
  system: {
    label: '系统消息',
    chip: '通用',
    chipVariant: 'accent',
    hint: '面向全体在线用户的通用广播。需标题和正文。',
  },
  update: {
    label: '更新通知',
    chip: '提示刷新',
    chipVariant: 'info',
    hint: '触发用户端 UpdateBanner 弹出，提示刷新页面获取新版本。可附加自定义文字。',
  },
  maintenance: {
    label: '维护通告',
    chip: '运维',
    chipVariant: 'warning',
    hint: '维护窗口前向用户预告。建议提前 24h 发，并配合 SettingsPage 维护模式 toggle。',
  },
};

const templates: Record<Tab, { title: string; message: string }[]> = {
  system: [
    { title: '功能上线公告', message: '我们新增了 [功能名] 功能，欢迎试用并反馈意见。' },
    { title: '账号安全提醒', message: '建议定期更换密码以保障账号安全。' },
  ],
  update: [
    { title: '', message: '有新版本可用，请刷新页面获取最新内容。' },
    { title: '', message: '为获得最佳体验，请刷新页面加载最新版本。' },
  ],
  maintenance: [
    { title: '维护通告', message: '系统将于 [时间] 进行维护升级，预计耗时 [时长]，期间服务可能不可用。' },
    { title: '紧急维护', message: '系统发现紧急问题需立即维护，预计 30 分钟内恢复，请保存当前学习进度。' },
  ],
};

export default function BroadcastPage() {
  const [tab, setTab] = createSignal<Tab>('system');
  const [title, setTitle] = createSignal('');
  const [message, setMessage] = createSignal('');
  const [sending, setSending] = createSignal(false);
  const [showConfirm, setShowConfirm] = createSignal(false);
  const [history, setHistory] = createSignal<SendHistoryEntry[]>([]);

  function pickTemplate(t: { title: string; message: string }) {
    setTitle(t.title);
    setMessage(t.message);
  }

  function onSendClick() {
    if (tab() === 'system' || tab() === 'maintenance') {
      if (!title().trim() || !message().trim()) {
        uiStore.toast.warning('请填写标题和内容');
        return;
      }
    }
    setShowConfirm(true);
  }

  async function doSend() {
    setShowConfirm(false);
    setSending(true);
    const current = tab();
    try {
      let sentCount: number | undefined;
      if (current === 'update') {
        const payload = message().trim() ? { message: message() } : undefined;
        await adminApi.broadcastUpdate(payload);
      } else {
        const res = await adminApi.broadcast({ title: title(), message: message() });
        sentCount = res.sent;
      }
      const ok = current === 'update' ? true : (sentCount ?? 0) >= 0;
      uiStore.toast.success(
        current === 'update' ? '更新通知已发送' : `已发送给 ${sentCount} 位用户`,
      );
      setHistory((h) => [
        { ts: new Date().toISOString(), tab: current, title: title() || undefined, message: message() || undefined, sentCount, ok },
        ...h,
      ].slice(0, 50));
      setTitle('');
      setMessage('');
    } catch (err: unknown) {
      uiStore.toast.error('发送失败', err instanceof Error ? err.message : '');
      setHistory((h) => [
        { ts: new Date().toISOString(), tab: current, title: title() || undefined, message: message() || undefined, ok: false },
        ...h,
      ].slice(0, 50));
    } finally {
      setSending(false);
    }
  }

  return (
    <div class="space-y-6">
      <HeroCard
        eyebrow="实时 SSE"
        eyebrowVariant="info"
        title="系统广播"
        desc="向在线用户推送系统消息、更新提醒与维护通告。三类广播 + 模板库 + 发送历史。"
      />

      <ConfirmDialog
        open={showConfirm()}
        title={`确认发送 ${tabMeta[tab()].label}`}
        message={
          <>
            <p class="mb-1">类型: <Badge variant={tabMeta[tab()].chipVariant}>{tabMeta[tab()].chip}</Badge></p>
            <Show when={title().trim()}>
              <p class="mb-1">标题: <span class="font-medium text-content">{title()}</span></p>
            </Show>
            <Show when={message().trim()}>
              <p class="mb-2 text-content-secondary text-sm">内容预览: {message().slice(0, 80)}{message().length > 80 ? '…' : ''}</p>
            </Show>
            <p>此消息将广播到所有在线客户端，确认发送？</p>
          </>
        }
        confirmText="确认发送"
        variant="warning"
        onConfirm={doSend}
        onCancel={() => setShowConfirm(false)}
      />

      <Card variant="elevated" padding="lg">
        <Tabs
          active={tab()}
          onChange={(v) => { setTab(v as Tab); setTitle(''); setMessage(''); }}
          tabs={[
            { id: 'system', label: tabMeta.system.label },
            { id: 'update', label: tabMeta.update.label },
            { id: 'maintenance', label: tabMeta.maintenance.label },
          ]}
        />
        <div class="mt-4 space-y-4 animate-tab-content-slide" data-tab={tab()}>
          <p class="text-sm text-content-tertiary">{tabMeta[tab()].hint}</p>

          {/* 模板库 */}
          <div>
            <SectionTitle as="h3" title="模板" sub="点击插入到表单" />
            <div class="flex flex-wrap gap-2">
              <For each={templates[tab()]}>{(t, i) => (
                <button
                  type="button"
                  onClick={() => pickTemplate(t)}
                  class="text-left max-w-xs px-3 py-2 rounded-md bg-surface-secondary hover:bg-surface-tertiary border border-border-hairline transition-colors duration-fast cursor-pointer"
                >
                  <div class="text-[12.5px] font-medium text-content truncate">
                    {t.title || `模板 ${i() + 1}`}
                  </div>
                  <div class="mt-0.5 text-[11.5px] text-content-tertiary line-clamp-2">{t.message}</div>
                </button>
              )}</For>
            </div>
          </div>

          <Show when={tab() !== 'update'}>
            <Input
              label="标题"
              value={title()}
              disabled={sending()}
              onInput={(e) => setTitle(e.currentTarget.value)}
              placeholder="通知标题"
            />
          </Show>
          <TextArea
            label={tab() === 'update' ? '提示文字（可选）' : '内容'}
            value={message()}
            disabled={sending()}
            onInput={(e) => setMessage(e.currentTarget.value)}
            placeholder={tab() === 'update' ? '有新版本可用，请刷新页面获取最新内容' : '通知内容'}
            rows={4}
          />
          <div class="flex items-center justify-between gap-3">
            <p class="text-xs text-content-tertiary">
              送达统计在发送完成后写入下方"发送历史"。
            </p>
            <Button
              onClick={onSendClick}
              loading={sending()}
              disabled={sending() || showConfirm()}
              variant={tab() === 'maintenance' ? 'warning' : 'primary'}
            >
              发送广播
            </Button>
          </div>
        </div>
      </Card>

      <Show when={history().length > 0}>
        <Card variant="elevated" padding="lg">
          <SectionTitle title="发送历史" sub={`最近 ${history().length} 条`} />
          <div class="divide-y divide-border-hairline">
            <For each={history()}>{(h) => (
              <div class="py-2.5 flex items-start gap-3">
                <Badge variant={h.ok ? tabMeta[h.tab].chipVariant : 'error'} size="sm">
                  {tabMeta[h.tab].chip}
                </Badge>
                <div class="flex-1 min-w-0">
                  <div class="text-sm font-medium text-content truncate">
                    {h.title || (h.tab === 'update' ? '（更新通知）' : '（无标题）')}
                  </div>
                  <Show when={h.message}>
                    <div class="mt-0.5 text-xs text-content-tertiary line-clamp-2">{h.message}</div>
                  </Show>
                </div>
                <div class="text-right shrink-0">
                  <div class="text-xs text-content-tertiary font-mono">{new Date(h.ts).toLocaleTimeString('zh-CN', { hour12: false })}</div>
                  <Show when={typeof h.sentCount === 'number'}>
                    <div class="text-xs text-success-strong tabular-nums">送达 {h.sentCount}</div>
                  </Show>
                  <Show when={!h.ok}>
                    <div class="text-xs text-error-strong">失败</div>
                  </Show>
                </div>
              </div>
            )}</For>
          </div>
        </Card>
      </Show>
    </div>
  );
}
