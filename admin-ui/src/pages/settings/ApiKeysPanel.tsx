import { createSignal, onMount, For, Show, createMemo } from 'solid-js';
import { adminApi, type ApiKey, type ApiKeyScope } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { SectionCard } from './SectionCard';
import { IconKeys, IconRotate, IconTrash } from './icons';

const scopeClass: Record<ApiKeyScope, string> = { read: 'r', write: 'w', admin: 'admin' };

function fmtExpiry(s?: string | null): { text: string; tone: 'ok' | 'warn' | 'err' } {
  if (!s) return { text: '永不过期', tone: 'ok' };
  const d = new Date(s);
  if (isNaN(+d)) return { text: s, tone: 'ok' };
  const days = Math.round((+d - Date.now()) / 86400000);
  if (days < 0) return { text: '已过期', tone: 'err' };
  if (days <= 14) return { text: `${days} 天后过期`, tone: 'warn' };
  return { text: d.toLocaleDateString('zh-CN'), tone: 'ok' };
}
const expiryColor: Record<string, string> = {
  ok: 'var(--success-strong)', warn: 'var(--warning-strong)', err: 'var(--error-strong)',
};

interface Props {
  ref?: (el: HTMLElement) => void;
  /** 即将过期数量回传给左侧 rail badge */
  onExpiringChange?: (n: number) => void;
}

export function ApiKeysPanel(props: Props) {
  const [keys, setKeys] = createSignal<ApiKey[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [showCreate, setShowCreate] = createSignal(false);
  const [name, setName] = createSignal('');
  const [scope, setScope] = createSignal<ApiKeyScope>('read');
  const [busy, setBusy] = createSignal(false);
  const [plaintext, setPlaintext] = createSignal<string | null>(null);
  const [delTarget, setDelTarget] = createSignal<ApiKey | null>(null);

  const expiringCount = createMemo(() => keys().filter((k) => fmtExpiry(k.expiresAt).tone === 'warn').length);

  async function load() {
    try {
      const res = await adminApi.apiKeys();
      setKeys(res.keys);
      props.onExpiringChange?.(res.keys.filter((k) => fmtExpiry(k.expiresAt).tone === 'warn').length);
    } catch (e) {
      uiStore.toast.error('加载 API 密钥失败', e instanceof Error ? e.message : '');
    } finally {
      setLoading(false);
    }
  }
  onMount(load);

  async function create() {
    if (!name().trim()) { uiStore.toast.warning('请填写密钥名称'); return; }
    setBusy(true);
    try {
      const res = await adminApi.createApiKey({ name: name().trim(), scope: scope() });
      setPlaintext(res.plaintext);
      uiStore.toast.success('密钥已生成，请立即复制');
      setShowCreate(false); setName('');
      await load();
    } catch (e) {
      uiStore.toast.error('生成失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  }

  async function rotate(k: ApiKey) {
    try {
      const res = await adminApi.rotateApiKey(k.id);
      setPlaintext(res.plaintext);
      uiStore.toast.success('密钥已轮换，旧密钥立即失效');
      await load();
    } catch (e) {
      uiStore.toast.error('轮换失败', e instanceof Error ? e.message : '');
    }
  }

  async function doDelete() {
    const k = delTarget();
    if (!k) return;
    try {
      await adminApi.deleteApiKey(k.id);
      setKeys((list) => list.filter((x) => x.id !== k.id));
      props.onExpiringChange?.(expiringCount());
      uiStore.toast.success('密钥已撤销');
    } catch (e) {
      uiStore.toast.error('撤销失败', e instanceof Error ? e.message : '');
    } finally {
      setDelTarget(null);
    }
  }

  // 即将过期数量通过 onExpiringChange 回传给父，用于左侧 rail badge

  return (
    <>
      <ConfirmDialog
        open={!!delTarget()}
        title="撤销 API 密钥"
        message={`确定撤销 "${delTarget()?.name}" 吗？所有使用该密钥的集成将立即失效。`}
        confirmText="撤销"
        variant="warning"
        onConfirm={doDelete}
        onCancel={() => setDelTarget(null)}
      />
      <SectionCard
        id="apikeys"
        ref={props.ref}
        icon={<IconKeys />}
        iconStyle={{ background: 'oklch(96% 0.04 75)', color: 'oklch(54% 0.20 75)' }}
        title="API 密钥"
        desc="第三方集成 / CLI / CI / webhook 验证用"
        right={<button class="st-btn st-btn-primary st-btn-sm" onClick={() => setShowCreate((v) => !v)}>+ 生成新密钥</button>}
      >
        <Show when={showCreate()}>
          <div class="st-row" style={{ 'border-bottom': '1px dashed var(--border-hairline)' }}>
            <div class="lbl"><strong>生成新密钥</strong><p>明文仅在创建时展示一次，请妥善保存。</p></div>
            <div class="ctrl">
              <div class="row">
                <input class="st-input" style={{ flex: 1 }} placeholder="密钥名称，如 github-actions-ci" value={name()} onInput={(e) => setName(e.currentTarget.value)} />
                <select class="st-input compact" style={{ width: '110px' }} value={scope()} onChange={(e) => setScope(e.currentTarget.value as ApiKeyScope)}>
                  <option value="read">read</option>
                  <option value="write">write</option>
                  <option value="admin">admin</option>
                </select>
                <button class="st-btn st-btn-primary st-btn-sm" disabled={busy()} onClick={create}>生成</button>
                <button class="st-btn st-btn-ghost st-btn-sm" onClick={() => setShowCreate(false)}>取消</button>
              </div>
            </div>
          </div>
        </Show>

        <Show when={plaintext()}>
          <div class="key-reveal">
            <div class="t">密钥已生成 —— 仅此一次可见，请立即复制</div>
            <code onClick={() => { navigator.clipboard?.writeText(plaintext()!); uiStore.toast.success('已复制到剪贴板'); }} style={{ cursor: 'pointer' }}>{plaintext()}</code>
          </div>
        </Show>

        <Show when={!loading()} fallback={<div class="st-skeleton" style={{ height: '120px' }} />}>
          <div class="key-row head">
            <span>名称 / 前缀</span>
            <span>权限</span>
            <span>过期</span>
            <span />
          </div>
          <Show when={keys().length} fallback={<p class="helper" style={{ padding: '14px 0' }}>暂无 API 密钥</p>}>
            <For each={keys()}>
              {(k) => {
                const exp = fmtExpiry(k.expiresAt);
                return (
                  <div class="key-row">
                    <div class="key-name">
                      <strong>{k.name}</strong>
                      <div class="id">{k.prefix || `key_${k.id}`}{k.revokedAt ? ' · 已撤销' : ''}</div>
                    </div>
                    <div class="scope-pills"><span class={scopeClass[k.scope]}>{k.scope}</span></div>
                    <div class="meta" style={{ color: k.revokedAt ? 'var(--content-tertiary)' : expiryColor[exp.tone] }}>{k.revokedAt ? '已撤销' : exp.text}</div>
                    <div class="actions">
                      <button title="轮换" disabled={!!k.revokedAt} onClick={() => rotate(k)}><IconRotate /></button>
                      <button title="撤销" class="danger" disabled={!!k.revokedAt} onClick={() => setDelTarget(k)}><IconTrash /></button>
                    </div>
                  </div>
                );
              }}
            </For>
          </Show>
        </Show>
      </SectionCard>
    </>
  );
}
