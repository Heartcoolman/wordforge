import { createResource, createSignal, Show, onCleanup, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Modal } from '@/components/ui/Modal';
import { Spinner } from '@/components/ui/Spinner';
import { adminApi } from '@/api/admin';
import { ApiError, connectSseStream } from '@/api/client';
import { uiStore } from '@/stores/ui';
import type { AdminUpdateStatus } from '@/types/admin';

// 仅 SSE 静默时回落轮询使用：SSE 正常推送时 progress 完全由事件驱动
const POLL_AFTER_APPLY_MS = 2000;
const POLL_TIMEOUT_MS = 300_000; // 5 分钟（异步化后给低性能机更宽容窗口）
// SSE 静默回落阈值：超过该窗口未收到 progress 才回落到 /status 轮询
const SSE_SILENCE_FALLBACK_MS = 10_000;

/// 把后端 apply task 的 phase 标识转成中文短句
const PHASE_LABEL: Record<string, string> = {
  pending: '等待启动',
  downloading: '下载中',
  verifying: '校验 SHA256',
  extracting: '解压产物',
  backing_up_db: '备份数据库',
  swapping: '替换二进制',
  restarting: '重启服务',
  completed: '完成',
  failed: '失败',
};

export default function UpdatesPage() {
  const [status, { refetch }] = createResource<AdminUpdateStatus>(() => adminApi.updatesStatus());
  const [checking, setChecking] = createSignal(false);
  const [applying, setApplying] = createSignal(false);
  const [confirmOpen, setConfirmOpen] = createSignal(false);
  const [progress, setProgress] = createSignal<{ phase: string; percent: number } | null>(null);
  // 终态标记：completed/failed 后 progress 保留显示（红/绿），不再被轮询清空
  const [terminal, setTerminal] = createSignal<'success' | 'failed' | null>(null);
  // SSE 最近一次推送时间，用于判断是否进入静默
  let lastSseAt = 0;

  // SSE: 订阅 release_available / update_progress；放进 onMount 避免 HMR/路由切换重复 connect
  onMount(() => {
    setProgress(null);
    const disconnect = connectSseStream({
      onReleaseAvailable: () => {
        void refetch();
        uiStore.toast.info('有新版本可用，已自动刷新');
      },
      onUpdateProgress: (p) => {
        lastSseAt = Date.now();
        setTerminal(null);
        setProgress(p);
      },
    });
    onCleanup(disconnect);
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
    setTerminal(null);
    lastSseAt = Date.now();
    setProgress({ phase: PHASE_LABEL.pending, percent: 0 });

    // v0.5.2+ apply 立即返回 202（异步执行），不再阻塞 handler 等到 exit
    try {
      await adminApi.updatesApply(s.latestVersion, s.currentVersion);
    } catch (err) {
      // 立即返回阶段的错误只可能是 4xx（参数 / 已在跑 / 版本不匹配等）
      if (err instanceof ApiError) {
        setApplying(false);
        setProgress(null);
        uiStore.toast.error(`升级启动失败 [${err.code}]`, err.message);
        await refetch();
        return;
      }
      // 网络问题（请求都没发出去），停下来让用户重试
      setApplying(false);
      setProgress(null);
      uiStore.toast.error('请求未送达', err instanceof Error ? err.message : '未知网络错误');
      return;
    }

    // apply 后优先信 SSE：onUpdateProgress 已经在 setProgress；
    // 仅当 SSE 静默超过 SSE_SILENCE_FALLBACK_MS 才回落到 /status 轮询；
    // SSE 推到 completed/failed 时主动 break。
    const deadline = Date.now() + POLL_TIMEOUT_MS;
    let outcome: 'success' | 'failed' | 'timeout' = 'timeout';
    let failureMessage: string | undefined;

    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, POLL_AFTER_APPLY_MS));

      // SSE 终态：phase 文案命中 completed/failed → 直接退出循环
      const p = progress();
      if (p) {
        if (p.phase === PHASE_LABEL.completed || p.percent >= 100) {
          outcome = 'success';
          break;
        }
        if (p.phase === PHASE_LABEL.failed) {
          outcome = 'failed';
          break;
        }
      }

      // SSE 仍活跃（最近 N 秒内有推送）→ 不回落轮询
      if (Date.now() - lastSseAt < SSE_SILENCE_FALLBACK_MS) continue;

      try {
        const fresh = await adminApi.updatesStatus();
        if (fresh.applyTask) {
          const label = PHASE_LABEL[fresh.applyTask.phase] ?? fresh.applyTask.phase;
          setProgress({ phase: label, percent: fresh.applyTask.percent });
          if (fresh.applyTask.error) {
            outcome = 'failed';
            failureMessage = fresh.applyTask.error;
            break;
          }
          if (fresh.applyTask.phase === 'completed') {
            outcome = 'success';
            break;
          }
        }
        if (fresh.currentVersion === s.latestVersion) {
          outcome = 'success';
          break;
        }
      } catch {
        // 重启窗口期 fetch 失败：保留当前 progress 文案，继续轮询
      }
    }

    setApplying(false);
    if (outcome === 'success') {
      setProgress({ phase: PHASE_LABEL.completed, percent: 100 });
      setTerminal('success');
      uiStore.toast.success(`升级成功，1.5 秒后自动刷新…（升级到 ${s.latestVersion}）`);
      setTimeout(() => window.location.reload(), 1500);
    } else if (outcome === 'failed') {
      // 保留最后一帧 progress（红色显示），不清空
      const last = progress();
      setProgress(last ? { phase: PHASE_LABEL.failed, percent: last.percent } : { phase: PHASE_LABEL.failed, percent: 0 });
      setTerminal('failed');
      uiStore.toast.error('升级失败', failureMessage ?? '后端 apply task 报错');
    } else {
      const last = progress();
      setProgress(last ? { phase: PHASE_LABEL.failed, percent: last.percent } : { phase: PHASE_LABEL.failed, percent: 0 });
      setTerminal('failed');
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
              <div class="grid grid-cols-1 md:grid-cols-2 gap-4 auto-rows-fr">
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
                  <h2 class="text-headline text-content">操作</h2>
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
                    远端发布了新版本，但 <strong>未找到匹配当前架构的产物</strong>。请检查 release.yml 是否覆盖此平台。
                  </p>
                </Show>

                <Show when={progress() && (applying() || terminal())}>
                  <div class="mt-4">
                    <div class="flex items-center justify-between text-sm mb-1">
                      <span class={terminal() === 'failed' ? 'text-error' : 'text-content-secondary'}>
                        {progress()!.phase}
                        <Show when={terminal() === 'failed'}>
                          <span class="ml-2 text-xs text-content-tertiary">（升级未完成）</span>
                        </Show>
                      </span>
                      <span class="text-content-tertiary font-mono">{progress()!.percent}%</span>
                    </div>
                    <div class="w-full bg-surface-secondary rounded-full h-2">
                      <div
                        class={`h-2 rounded-full transition-[width] duration-300 ease-out ${terminal() === 'failed' ? 'bg-error' : 'bg-accent'}`}
                        style={{ width: `${progress()!.percent}%` }}
                      />
                    </div>
                  </div>
                </Show>
              </Card>

              <Show when={s().releaseNotes}>
                <Card>
                  <div class="flex items-center justify-between mb-3">
                    <h2 class="text-headline text-content">Release Notes</h2>
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
                  <pre class="whitespace-pre-wrap text-sm text-content-secondary font-mono leading-relaxed max-h-96 overflow-y-auto">
                    {s().releaseNotes}
                  </pre>
                </Card>
              </Show>

              <Card>
                <h3 class="text-sm font-semibold text-content mb-2">安全提示</h3>
                <ul class="text-xs text-content-tertiary space-y-1 list-disc pl-5">
                  <li>升级前会先 VACUUM INTO 备份当前数据库到 <code class="font-mono text-content">data/learning-{s().currentVersion}.backup.db</code></li>
                  <li>旧二进制会保留 2 份在安装目录（<code class="font-mono text-content">wordforge.{s().currentVersion}</code>）以便手动回滚</li>
                  <li>下载产物会校验 sha256；不匹配直接拒绝</li>
                  <li>当前裸跑模式下，进程通过 fork-exec 自重启；如果有 systemd / supervisor，请确保它们配置了 <code class="font-mono text-content">Restart=on-failure</code></li>
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
