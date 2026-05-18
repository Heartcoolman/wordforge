import { createResource, createSignal, Show, onCleanup, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Modal } from '@/components/ui/Modal';
import { Spinner } from '@/components/ui/Spinner';
import { adminApi } from '@/api/admin';
import { ApiError, connectSseStream } from '@/api/client';
import { uiStore } from '@/stores/ui';
import type { AdminUpdateStatus } from '@/types/admin';

const POLL_AFTER_APPLY_MS = 2000;
const POLL_TIMEOUT_MS = 120_000;

export default function UpdatesPage() {
  const [status, { refetch }] = createResource<AdminUpdateStatus>(() => adminApi.updatesStatus());
  const [checking, setChecking] = createSignal(false);
  const [applying, setApplying] = createSignal(false);
  const [confirmOpen, setConfirmOpen] = createSignal(false);
  const [progress, setProgress] = createSignal<{ phase: string; percent: number } | null>(null);

  // SSE: 订阅 release_available / update_progress
  const disconnect = connectSseStream({
    onReleaseAvailable: () => {
      void refetch();
      uiStore.toast.info('有新版本可用，已自动刷新');
    },
    onUpdateProgress: (p) => setProgress(p),
  });
  onCleanup(disconnect);

  // 进度推送只在 applying 时显示；apply 结束后清空
  onMount(() => {
    setProgress(null);
  });

  async function handleCheck() {
    setChecking(true);
    try {
      const fresh = await adminApi.updatesCheck();
      if (fresh.hasUpdate) {
        uiStore.toast.success(`发现新版本 ${fresh.latestVersion}`);
      } else {
        uiStore.toast.info(`当前已是最新版本（${fresh.currentVersion}）`);
      }
      await refetch();
    } catch (e) {
      uiStore.toast.error('检查失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setChecking(false);
    }
  }

  function openConfirm() {
    const s = status();
    if (!s || !s.latestVersion || !s.canApply) return;
    setConfirmOpen(true);
  }

  async function confirmApply() {
    const s = status();
    if (!s || !s.latestVersion) return;
    setConfirmOpen(false);
    setApplying(true);
    setProgress({ phase: 'starting', percent: 0 });

    try {
      // POST /apply 在成功路径会触发服务端 process::exit(0)，
      // fetch 极可能以网络中断的方式返回错误 —— 这是预期的"成功信号"。
      await adminApi.updatesApply(s.latestVersion, s.currentVersion);
      // 罕见路径：apply 返回了 JSON（说明 fork-exec 提前失败），继续走 polling
    } catch (err) {
      // Codex P2 (2nd pass): 区分网络中断（=服务端 exit 成功）vs 4xx/5xx（=真实失败）。
      // ApiError.status === 0 是 client.ts 给 NETWORK_ERROR / TIMEOUT 的占位码。
      const isServerSideFailure = err instanceof ApiError && err.status >= 400;
      if (isServerSideFailure) {
        setApplying(false);
        setProgress(null);
        const e = err as ApiError;
        uiStore.toast.error(`升级失败 [${e.code}]`, e.message);
        await refetch();
        return;
      }
      // 否则视为重启进行中，继续走下面的 polling
    }

    // Poll /api/status 直到 version 切到 latest（5s 间隔，2 min 上限）
    const deadline = Date.now() + POLL_TIMEOUT_MS;
    let success = false;
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, POLL_AFTER_APPLY_MS));
      try {
        const fresh = await adminApi.updatesStatus();
        if (fresh.currentVersion === s.latestVersion) {
          success = true;
          break;
        }
      } catch {
        // 服务端尚未拉起，继续轮询
      }
    }

    setApplying(false);
    setProgress(null);
    if (success) {
      uiStore.toast.success(`已升级到 ${s.latestVersion}，刷新页面以加载新前端`);
      setTimeout(() => window.location.reload(), 1500);
    } else {
      uiStore.toast.error('升级超时', '请检查后端日志，或 SSH 登录手动验证状态');
    }
    await refetch();
  }

  return (
    <div class="space-y-6">
      <Show when={!status.loading} fallback={<Spinner />}>
        <Show when={status()}>
          {(s) => (
            <>
              <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                <Card>
                  <p class="text-sm text-content-secondary mb-1">当前版本</p>
                  <p class="text-2xl font-semibold text-content">{s().currentVersion}</p>
                  <p class="text-xs text-content-tertiary mt-2">
                    自动检查：{s().autoCheckEnabled ? '已开启（每小时）' : '已关闭'}
                  </p>
                </Card>
                <Card>
                  <p class="text-sm text-content-secondary mb-1">远端最新</p>
                  <p class="text-2xl font-semibold text-content">
                    {s().latestVersion ?? '尚未检查'}
                  </p>
                  <p class="text-xs text-content-tertiary mt-2">
                    {s().lastCheckedAt
                      ? `最近检查：${new Date(s().lastCheckedAt!).toLocaleString('zh-CN')}`
                      : '从未检查'}
                  </p>
                </Card>
              </div>

              <Card>
                <div class="flex items-center justify-between mb-4">
                  <h2 class="text-lg font-semibold text-content">操作</h2>
                  <div class="flex gap-2">
                    <Button variant="ghost" onClick={handleCheck} loading={checking()}>
                      立即检查
                    </Button>
                    <Button
                      onClick={openConfirm}
                      disabled={
                        !s().hasUpdate || !s().canApply || applying() || !s().latestVersion
                      }
                      loading={applying()}
                    >
                      一键更新到 {s().latestVersion ?? '...'}
                    </Button>
                  </div>
                </div>

                <Show when={!s().canApply && s().hasUpdate}>
                  <p class="text-sm text-warning">
                    远端发布了新版本，但**未找到匹配当前架构的产物**。请检查 release.yml 是否覆盖此平台。
                  </p>
                </Show>

                <Show when={applying() && progress()}>
                  <div class="mt-4">
                    <div class="flex items-center justify-between text-sm mb-1">
                      <span class="text-content-secondary">{progress()!.phase}</span>
                      <span class="text-content-tertiary font-mono">{progress()!.percent}%</span>
                    </div>
                    <div class="w-full bg-surface-secondary rounded-full h-2">
                      <div
                        class="bg-accent h-2 rounded-full transition-all duration-300"
                        style={{ width: `${progress()!.percent}%` }}
                      />
                    </div>
                  </div>
                </Show>
              </Card>

              <Show when={s().releaseNotes}>
                <Card>
                  <div class="flex items-center justify-between mb-3">
                    <h2 class="text-lg font-semibold text-content">Release Notes</h2>
                    <Show when={s().releaseUrl}>
                      <a
                        href={s().releaseUrl!}
                        target="_blank"
                        rel="noopener"
                        class="text-sm text-accent hover:underline"
                      >
                        在 GitHub 打开 ↗
                      </a>
                    </Show>
                  </div>
                  <pre class="whitespace-pre-wrap text-sm text-content-secondary font-mono leading-relaxed">
                    {s().releaseNotes}
                  </pre>
                </Card>
              </Show>

              <Card>
                <h3 class="text-sm font-semibold text-content mb-2">安全提示</h3>
                <ul class="text-xs text-content-tertiary space-y-1 list-disc pl-5">
                  <li>升级前会先 VACUUM INTO 备份当前数据库到 `data/learning-{s().currentVersion}.backup.db`</li>
                  <li>旧二进制会保留 2 份在安装目录（`wordforge.{s().currentVersion}`）以便手动回滚</li>
                  <li>下载产物会校验 sha256；不匹配直接拒绝</li>
                  <li>当前裸跑模式下，进程通过 fork-exec 自重启；如果有 systemd / supervisor，请确保它们配置了 `Restart=on-failure`</li>
                </ul>
              </Card>
            </>
          )}
        </Show>
      </Show>

      <Modal open={confirmOpen()} onClose={() => setConfirmOpen(false)} title="确认一键更新">
        <Show when={status()}>
          {(s) => (
            <div class="space-y-4">
              <p class="text-sm text-content-secondary">
                即将从 <span class="font-mono text-content">{s().currentVersion}</span> 升级到{' '}
                <span class="font-mono text-content font-semibold">{s().latestVersion}</span>。
              </p>
              <ul class="text-sm text-content-secondary space-y-1 list-disc pl-5">
                <li>会先备份数据库</li>
                <li>替换二进制与前端 static 目录</li>
                <li>fork-exec 自重启（HTTP 连接会短暂中断）</li>
                <li>升级期间不要刷新页面或重启进程</li>
              </ul>
              <div class="flex justify-end gap-2 pt-2">
                <Button variant="ghost" onClick={() => setConfirmOpen(false)}>
                  取消
                </Button>
                <Button onClick={confirmApply}>开始升级</Button>
              </div>
            </div>
          )}
        </Show>
      </Modal>
    </div>
  );
}
