import { createSignal, createMemo, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Input } from '@/components/ui/Input';
import { Tabs } from '@/components/ui/Tabs';
import { ProgressBar } from '@/components/ui/Progress';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Skeleton } from '@/components/ui/Skeleton';
import { Empty } from '@/components/ui/Empty';
import { HeroCard } from '@/components/ui/HeroCard';
import { SectionTitle } from '@/components/ui/SectionTitle';
import { uiStore } from '@/stores/ui';
import { packsApi, type AdminPackEntry, type PackChannel, type ResourcePackVersion } from '@/api/packs';

/**
 * 资源包管理页。
 * 多通道（stable / beta）发布、版本上传（XHR 进度）、激活 / 软删除、签名校验提示。
 *
 * 对接 /api/admin/resource-packs（详见 api/packs.ts）。
 */
export default function ResourcePacksPage() {
  const [loading, setLoading] = createSignal(true);
  const [entries, setEntries] = createSignal<AdminPackEntry[]>([]);
  const [channel, setChannel] = createSignal<PackChannel>('stable');
  const [selectedPackId, setSelectedPackId] = createSignal<string | null>(null);

  // 上传表单
  const [uploadVersion, setUploadVersion] = createSignal('');
  const [uploadMinApp, setUploadMinApp] = createSignal('');
  const [uploadDesc, setUploadDesc] = createSignal('');
  const [uploadFile, setUploadFile] = createSignal<File | null>(null);
  const [uploadProgress, setUploadProgress] = createSignal(0);
  const [uploading, setUploading] = createSignal(false);

  // 激活 / 删除 确认
  const [activateTarget, setActivateTarget] = createSignal<ResourcePackVersion | null>(null);
  const [deleteTarget, setDeleteTarget] = createSignal<ResourcePackVersion | null>(null);

  async function refresh() {
    setLoading(true);
    try {
      const list = await packsApi.list();
      setEntries(list);
      if (list.length > 0 && !selectedPackId()) setSelectedPackId(list[0].packId);
    } catch (err) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  onMount(refresh);

  const currentPack = createMemo(() => entries().find((e) => e.packId === selectedPackId()));
  const currentVersions = createMemo(() => {
    const p = currentPack();
    if (!p) return [];
    return p.versions
      .filter((v) => v.channel === channel())
      .sort((a, b) => (b.publishedAt > a.publishedAt ? 1 : -1));
  });
  const activeVersion = createMemo(() =>
    currentVersions().find((v) => v.deactivatedAt == null),
  );

  async function doUpload() {
    const pack = currentPack();
    if (!pack || !uploadFile() || !uploadVersion().trim()) {
      uiStore.toast.warning('请选择资源包、填写版本号并选择文件');
      return;
    }
    setUploading(true);
    setUploadProgress(0);
    try {
      await packsApi.uploadVersion(
        pack.packId,
        {
          version: uploadVersion().trim(),
          channel: channel(),
          minAppVersion: uploadMinApp().trim() || undefined,
          description: uploadDesc().trim() || undefined,
        },
        uploadFile()!,
        (frac) => setUploadProgress(Math.round(frac * 100)),
      );
      uiStore.toast.success('上传成功，已自动签名落盘');
      setUploadVersion('');
      setUploadMinApp('');
      setUploadDesc('');
      setUploadFile(null);
      setUploadProgress(0);
      await refresh();
    } catch (err) {
      uiStore.toast.error('上传失败', err instanceof Error ? err.message : '');
    } finally {
      setUploading(false);
    }
  }

  async function doActivate() {
    const v = activateTarget();
    const pack = currentPack();
    if (!v || !pack) return;
    setActivateTarget(null);
    try {
      await packsApi.setActive(pack.packId, v.channel, v.version);
      uiStore.toast.success(`已激活 ${pack.packId} ${v.version}（${v.channel}），已 SSE 广播`);
      await refresh();
    } catch (err) {
      uiStore.toast.error('激活失败', err instanceof Error ? err.message : '');
    }
  }

  async function doDeactivate() {
    const v = deleteTarget();
    const pack = currentPack();
    if (!v || !pack) return;
    setDeleteTarget(null);
    try {
      await packsApi.deactivateVersion(pack.packId, v.version);
      uiStore.toast.success('已停用版本');
      await refresh();
    } catch (err) {
      uiStore.toast.error('停用失败', err instanceof Error ? err.message : '');
    }
  }

  const formatSize = (b: number) => {
    if (b < 1024) return `${b} B`;
    if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
    return `${(b / 1024 / 1024).toFixed(2)} MB`;
  };

  return (
    <div class="space-y-6">
      <HeroCard
        eyebrow="多通道 + 签名"
        eyebrowVariant="accent"
        title="资源包"
        desc="管理多通道（stable / beta）资源包发布。上传自动 SHA256 + Ed25519 签名落盘，激活后 SSE 广播到所有在线客户端。"
        meta={[
          { value: entries().length, label: '包数量' },
          { value: entries().reduce((acc, e) => acc + e.versions.length, 0), label: '总版本数' },
          { value: entries().reduce((acc, e) => acc + e.versions.filter((v) => v.channel === 'stable' && !v.deactivatedAt).length, 0), label: 'Stable 激活' },
          { value: entries().reduce((acc, e) => acc + e.versions.filter((v) => v.channel === 'beta' && !v.deactivatedAt).length, 0), label: 'Beta 激活' },
        ]}
      />

      <ConfirmDialog
        open={!!activateTarget()}
        title="确认激活版本"
        message={
          <p>
            激活 <span class="font-mono text-content">{activateTarget()?.version}</span> 至{' '}
            <Badge variant={activateTarget()?.channel === 'beta' ? 'warning' : 'success'} size="sm">
              {activateTarget()?.channel}
            </Badge>{' '}
            通道？激活后会立即通过 SSE 广播到所有在线客户端。
          </p>
        }
        confirmText="确认激活"
        variant="warning"
        onConfirm={doActivate}
        onCancel={() => setActivateTarget(null)}
      />

      <ConfirmDialog
        open={!!deleteTarget()}
        title="确认停用版本"
        message={
          <p>
            停用 <span class="font-mono text-content">{deleteTarget()?.version}</span>{' '}
            后将从客户端 manifest 中摘除（已下载客户端不受影响，软删除可恢复）。
          </p>
        }
        confirmText="确认停用"
        variant="warning"
        onConfirm={doDeactivate}
        onCancel={() => setDeleteTarget(null)}
      />

      <Show
        when={!loading()}
        fallback={
          <div class="space-y-3">
            <Skeleton height="3rem" />
            <Skeleton height="3rem" />
            <Skeleton height="3rem" />
          </div>
        }
      >
        <Show when={entries().length > 0} fallback={<Empty title="暂无资源包" />}>
          {/* Pack 切换 */}
          <Card variant="elevated" padding="md">
            <div class="flex items-center gap-3 flex-wrap">
              <span class="text-sm font-medium text-content-secondary">资源包：</span>
              <For each={entries()}>{(e) => (
                <button
                  type="button"
                  onClick={() => setSelectedPackId(e.packId)}
                  class={
                    e.packId === selectedPackId()
                      ? 'px-3 py-1.5 rounded-md bg-accent text-accent-content text-sm font-medium cursor-pointer'
                      : 'px-3 py-1.5 rounded-md bg-surface-secondary text-content-secondary hover:bg-surface-tertiary text-sm font-medium cursor-pointer'
                  }
                >
                  <span class="font-mono">{e.packId}</span>
                  <span class="ml-1.5 text-[11px] opacity-70">{e.versions.length}</span>
                </button>
              )}</For>
            </div>
          </Card>

          {/* 通道切换 */}
          <Card variant="elevated" padding="none">
            <Tabs
              active={channel()}
              onChange={(c) => setChannel(c as PackChannel)}
              tabs={[
                { id: 'stable', label: 'Stable' },
                { id: 'beta', label: 'Beta' },
              ]}
            />
            <div class="p-5">
              {/* 当前激活版本卡 */}
              <Show
                when={activeVersion()}
                fallback={
                  <p class="text-sm text-content-tertiary py-4">
                    当前 {channel()} 通道没有激活版本。上传新版后将自动作为最新候选。
                  </p>
                }
              >
                {(v) => (
                  <div class="mb-5 p-4 rounded-lg bg-accent-light/40 border border-accent/20">
                    <div class="flex items-baseline justify-between gap-3 flex-wrap">
                      <div class="flex items-baseline gap-2">
                        <span class="text-xs uppercase tracking-wide text-accent">当前激活</span>
                        <span class="font-mono text-lg font-semibold text-content">{v().version}</span>
                        <Badge variant="success" size="sm">已签名</Badge>
                      </div>
                      <div class="text-xs text-content-tertiary font-mono">
                        {new Date(v().publishedAt).toLocaleString('zh-CN', { hour12: false })}
                      </div>
                    </div>
                    <div class="mt-2 grid grid-cols-2 md:grid-cols-4 gap-2 text-xs">
                      <div><span class="text-content-tertiary">SHA256: </span><span class="font-mono truncate" title={v().sha256}>{v().sha256.slice(0, 12)}…</span></div>
                      <div><span class="text-content-tertiary">大小: </span><span class="font-mono">{formatSize(v().sizeBytes)}</span></div>
                      <Show when={v().minAppVersion}><div><span class="text-content-tertiary">最低 app: </span><span class="font-mono">{v().minAppVersion}</span></div></Show>
                      <div><span class="text-content-tertiary">算法: </span><span class="font-mono">{v().signatureAlg}</span></div>
                    </div>
                  </div>
                )}
              </Show>

              {/* 版本列表 */}
              <SectionTitle title={`${channel()} 历史版本`} sub={`${currentVersions().length} 个`} />
              <Show when={currentVersions().length > 0} fallback={<Empty title={`${channel()} 通道暂无版本`} />}>
                <div class="divide-y divide-border-hairline">
                  <For each={currentVersions()}>{(v) => (
                    <div class="py-3 flex items-start gap-3 animate-list-item-enter">
                      <span class={
                        v.deactivatedAt
                          ? 'mt-1 size-2 rounded-full bg-content-tertiary shrink-0'
                          : v === activeVersion()
                            ? 'mt-1 size-2 rounded-full bg-accent shrink-0 animate-ring-pulse'
                            : 'mt-1 size-2 rounded-full bg-success shrink-0'
                      } />
                      <div class="flex-1 min-w-0">
                        <div class="flex items-baseline gap-2 flex-wrap">
                          <span class="font-mono font-medium text-content">{v.version}</span>
                          <Show when={v.deactivatedAt}><Badge variant="default" size="sm">已停用</Badge></Show>
                          <Show when={v === activeVersion()}><Badge variant="accent" size="sm">激活中</Badge></Show>
                        </div>
                        <div class="mt-0.5 text-xs text-content-tertiary font-mono">
                          {new Date(v.publishedAt).toLocaleString('zh-CN', { hour12: false })} · {formatSize(v.sizeBytes)} · sha256:{v.sha256.slice(0, 8)}…
                        </div>
                      </div>
                      <div class="flex items-center gap-1.5 shrink-0">
                        <Show when={v !== activeVersion() && !v.deactivatedAt}>
                          <Button size="sm" variant="ghost" onClick={() => setActivateTarget(v)}>激活</Button>
                        </Show>
                        <Show when={!v.deactivatedAt}>
                          <Button size="sm" variant="ghost" onClick={() => setDeleteTarget(v)}>停用</Button>
                        </Show>
                      </div>
                    </div>
                  )}</For>
                </div>
              </Show>
            </div>
          </Card>

          {/* 上传新版 */}
          <Card variant="elevated" padding="lg">
            <SectionTitle title="上传新版" sub={`上传到 ${currentPack()?.packId ?? '?'} / ${channel()} 通道`} />
            <div class="grid md:grid-cols-2 gap-4">
              <Input
                label="版本号"
                value={uploadVersion()}
                disabled={uploading()}
                onInput={(e) => setUploadVersion(e.currentTarget.value)}
                placeholder="如 1.2.0 或 2026-05-28"
              />
              <Input
                label="最低 app 版本（可选）"
                value={uploadMinApp()}
                disabled={uploading()}
                onInput={(e) => setUploadMinApp(e.currentTarget.value)}
                placeholder="如 0.6.0"
              />
              <Input
                label="描述（可选）"
                value={uploadDesc()}
                disabled={uploading()}
                onInput={(e) => setUploadDesc(e.currentTarget.value)}
                placeholder="本版本主要变化"
                class="md:col-span-2"
              />
              <div class="md:col-span-2">
                <label class="text-xs font-medium text-content-secondary mb-1.5 block">payload 文件</label>
                <input
                  type="file"
                  accept="application/json,application/octet-stream,.json"
                  disabled={uploading()}
                  onChange={(e) => setUploadFile(e.currentTarget.files?.[0] ?? null)}
                  class="text-sm file:mr-3 file:px-3 file:py-1.5 file:rounded-md file:bg-accent-light file:text-accent file:border-0 file:cursor-pointer file:text-xs file:font-medium"
                />
                <Show when={uploadFile()}>
                  <p class="mt-1.5 text-xs text-content-tertiary font-mono">
                    {uploadFile()!.name} · {formatSize(uploadFile()!.size)}
                  </p>
                </Show>
              </div>
              <Show when={uploading()}>
                <div class="md:col-span-2">
                  <ProgressBar value={uploadProgress()} max={100} />
                  <p class="mt-1 text-xs text-content-tertiary text-center font-mono">{uploadProgress()}%</p>
                </div>
              </Show>
              <div class="md:col-span-2">
                <Button
                  onClick={doUpload}
                  loading={uploading()}
                  disabled={uploading() || !uploadFile() || !uploadVersion().trim()}
                >
                  上传并签名
                </Button>
              </div>
            </div>
          </Card>
        </Show>
      </Show>
    </div>
  );
}
