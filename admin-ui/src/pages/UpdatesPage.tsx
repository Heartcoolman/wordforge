import { createMemo, createResource, createSignal, Show, For, onCleanup, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { Spinner } from '@/components/ui/Spinner';
import { Switch } from '@/components/ui/Switch';
import { Collapsible } from '@/components/ui/Collapsible';
import { HeroCard } from '@/components/ui/HeroCard';
import { UpdateChannelCard } from '@/components/admin/UpdateChannelCard';
import { adminApi } from '@/api/admin';
import { ApiError, connectSseStream } from '@/api/http';
import { uiStore } from '@/stores/ui';
import type { AdminUpdateStatus, ChannelStatus } from '@/types/admin';

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

const CHANNEL_LABEL: Record<'stable' | 'beta', string> = {
  stable: '稳定通道',
  beta: 'Beta 通道',
};

const OUTCOME_LABEL: Record<string, string> = {
  success: '成功',
  failed: '失败',
  in_progress: '进行中',
};

export default function UpdatesPage() {
  const [status, { refetch }] = createResource<AdminUpdateStatus>(() =>
    adminApi.updatesStatus(),
  );
  // S5：升级历史列表
  const [history, { refetch: refetchHistory }] = createResource(() =>
    adminApi.updatesHistory(),
  );
  // 维护模式状态:来自 SystemSettings.maintenanceMode
  const [settings, { refetch: refetchSettings }] = createResource(() => adminApi.getSettings());
  const [maintenanceBusy, setMaintenanceBusy] = createSignal(false);
  const [checking, setChecking] = createSignal(false);
  const [applying, setApplying] = createSignal(false);
  const [pendingChannel, setPendingChannel] = createSignal<'stable' | 'beta' | null>(null);
  const [confirmOpen, setConfirmOpen] = createSignal(false);
  // 回滚到上一版:从 history 找最新 success entry 的 fromVersion
  const [rollbackOpen, setRollbackOpen] = createSignal(false);
  const [rollbackBusy, setRollbackBusy] = createSignal(false);
  const [progress, setProgress] = createSignal<{ phase: string; percent: number } | null>(null);
  // 终态标记：completed/failed 后 progress 保留显示(红/绿),不再被轮询清空
  const [terminal, setTerminal] = createSignal<'success' | 'failed' | null>(null);
  // SSE 最近一次推送时间,用于判断是否进入静默
  let lastSseAt = 0;

  // 从 history 推导上一版:最新 success 的 fromVersion(必须不等于当前版本)
  const previousVersion = createMemo<string | null>(() => {
    const entries = history()?.entries ?? [];
    const current = status()?.currentVersion;
    if (!current) return null;
    const lastSuccess = entries.find(
      (e) => e.outcome === 'success' && e.fromVersion && e.fromVersion !== current,
    );
    return lastSuccess?.fromVersion ?? null;
  });

  // history 成功率:历史总数 vs success 数
  const historyStats = createMemo(() => {
    const entries = history()?.entries ?? [];
    if (entries.length === 0) return { total: 0, success: 0, rate: 0 };
    const success = entries.filter((e) => e.outcome === 'success').length;
    return { total: entries.length, success, rate: Math.round((success / entries.length) * 100) };
  });

  async function toggleMaintenance(active: boolean) {
    setMaintenanceBusy(true);
    try {
      await adminApi.setMaintenance(active);
      await refetchSettings();
      uiStore.toast.success(active ? '维护模式已开启' : '维护模式已关闭');
    } catch (e) {
      uiStore.toast.error('维护模式切换失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setMaintenanceBusy(false);
    }
  }

  async function confirmRollback() {
    const prev = previousVersion();
    const s = status();
    if (!prev || !s) return;
    setRollbackOpen(false);
    setRollbackBusy(true);
    try {
      // 通过 apply 端点切到 prev 版本(后端可能拒绝向下迁移,此时返回错误)
      // channel 默认走 stable;后端通过 apply payload 中的 targetVersion 直接覆盖
      await adminApi.updatesApply('stable', prev, s.currentVersion);
      uiStore.toast.success(`已下发回滚到 ${prev},等待重启...`);
      setTimeout(() => window.location.reload(), 2000);
    } catch (e) {
      const msg = e instanceof Error ? e.message : '未知错误';
      uiStore.toast.error('回滚被后端拒绝',
        msg.includes('semver') || msg.includes('forbidden')
          ? '后端策略要求版本只能向上迁移,请 SSH 手动 swap 二进制'
          : msg,
      );
    } finally {
      setRollbackBusy(false);
    }
  }

  // SSE: 订阅 release_available / update_progress；放进 onMount 避免 HMR/路由切换重复 connect
  onMount(() => {
    setProgress(null);
    const disconnect = connectSseStream({
      onReleaseAvailable: (payload) => {
        void refetch();
        uiStore.toast.info(
          `${CHANNEL_LABEL[payload.channel]} 有新版本 ${payload.latestTag}，已刷新`,
        );
      },
      onUpdateProgress: (p) => {
        lastSseAt = Date.now();
        setTerminal(null);
        setProgress(p);
      },
    });
    onCleanup(disconnect);
  });

  /** 当前部署是否还应该提示「有更新」（任一通道 hasUpdate=true 即有） */
  const anyHasUpdate = (s: AdminUpdateStatus | undefined): boolean =>
    !!(s?.stable?.hasUpdate || s?.beta?.hasUpdate);

  async function handleCheck() {
    setChecking(true);
    try {
      const fresh = await adminApi.updatesCheck();
      const tags: string[] = [];
      if (fresh.stable?.hasUpdate) tags.push(`stable ${fresh.stable.latestVersion}`);
      if (fresh.beta?.hasUpdate) tags.push(`beta ${fresh.beta.latestVersion}`);
      if (tags.length > 0) {
        uiStore.toast.success(`发现新版本：${tags.join('、')}`);
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

  function openConfirm(channel: 'stable' | 'beta') {
    const s = status();
    if (!s) return;
    const ch = channel === 'stable' ? s.stable : s.beta;
    if (!ch || !ch.canApply) return;
    setPendingChannel(channel);
    setConfirmOpen(true);
  }

  function currentChannelStatus(): ChannelStatus | null {
    const ch = pendingChannel();
    const s = status();
    if (!ch || !s) return null;
    return ch === 'stable' ? s.stable : s.beta;
  }

  async function confirmApply() {
    const s = status();
    const ch = pendingChannel();
    const target = currentChannelStatus();
    if (!s || !ch || !target) return;
    setConfirmOpen(false);
    setApplying(true);
    setTerminal(null);
    lastSseAt = Date.now();
    setProgress({ phase: PHASE_LABEL.pending, percent: 0 });

    // v0.5.2+ apply 立即返回 202（异步执行），不再阻塞 handler 等到 exit
    try {
      await adminApi.updatesApply(ch, target.latestVersion, s.currentVersion);
    } catch (err) {
      // 立即返回阶段的错误只可能是 4xx（参数 / 已在跑 / 版本不匹配等）
      if (err instanceof ApiError) {
        setApplying(false);
        setProgress(null);
        uiStore.toast.error(`升级启动失败 [${err.code}]`, err.message);
        await refetch();
        return;
      }
      setApplying(false);
      setProgress(null);
      uiStore.toast.error('请求未送达', err instanceof Error ? err.message : '未知网络错误');
      return;
    }

    // apply 后优先信 SSE；SSE 静默 >10s 才回落 /status 轮询
    const deadline = Date.now() + POLL_TIMEOUT_MS;
    let outcome: 'success' | 'failed' | 'timeout' = 'timeout';
    let failureMessage: string | undefined;

    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, POLL_AFTER_APPLY_MS));

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
        if (fresh.currentVersion === target.latestVersion) {
          outcome = 'success';
          break;
        }
      } catch {
        // 重启窗口期 fetch 失败：保留当前 progress 文案，继续轮询
      }
    }

    setApplying(false);
    setPendingChannel(null);
    if (outcome === 'success') {
      setProgress({ phase: PHASE_LABEL.completed, percent: 100 });
      setTerminal('success');
      uiStore.toast.success(`升级成功，1.5 秒后自动刷新…（升级到 ${target.latestVersion}）`);
      setTimeout(() => window.location.reload(), 1500);
    } else if (outcome === 'failed') {
      const last = progress();
      setProgress(
        last ? { phase: PHASE_LABEL.failed, percent: last.percent } : { phase: PHASE_LABEL.failed, percent: 0 },
      );
      setTerminal('failed');
      uiStore.toast.error('升级失败', failureMessage ?? '后端 apply task 报错');
    } else {
      const last = progress();
      setProgress(
        last ? { phase: PHASE_LABEL.failed, percent: last.percent } : { phase: PHASE_LABEL.failed, percent: 0 },
      );
      setTerminal('failed');
      uiStore.toast.error('升级超时', '请检查后端日志，或 SSH 登录手动验证状态');
    }
    await refetch();
  }

  return (
    <div class="space-y-4">
      <Show when={!status.loading} fallback={<Spinner />}>
        <Show when={status()}>
          {(s) => (
            <>
              {/* Hero —— 版本运维总览(当前版本 / 自动检查 / 历史成功率 / 维护模式) */}
              <HeroCard
                eyebrow={s().stable?.hasUpdate || s().beta?.hasUpdate ? '有可用更新' : '当前最新'}
                eyebrowVariant={s().stable?.hasUpdate || s().beta?.hasUpdate ? 'warning' : 'success'}
                title="版本与自更新"
                desc="一键升级走 GitHub Release · VACUUM INTO 数据库备份 · sha256 校验 · fork-exec 自重启。维护模式开启时,客户端 /api/* 全部返回 503。"
                cta={
                  <div class="flex flex-wrap items-center gap-2">
                    <Show when={previousVersion()}>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setRollbackOpen(true)}
                        loading={rollbackBusy()}
                      >
                        回到 {previousVersion()}
                      </Button>
                    </Show>
                    <Button variant="ghost" size="sm" onClick={handleCheck} loading={checking()}>
                      立即检查
                    </Button>
                  </div>
                }
                meta={[
                  { value: s().currentVersion, label: '当前版本' },
                  { value: s().autoCheckEnabled ? '每小时' : '已关闭', label: '自动检查' },
                  { value: history() ? `${historyStats().rate}%` : '—', label: '历史成功率' },
                  {
                    value: settings()?.maintenanceMode ? '已开启' : '关闭',
                    label: '维护模式',
                  },
                ]}
              />

              {/* 维护模式 toggle + 上次检查时间 */}
              <Card>
                <div class="flex flex-wrap items-center justify-between gap-3">
                  <div class="flex items-center gap-4">
                    <Switch
                      checked={!!settings()?.maintenanceMode}
                      onChange={(v) => toggleMaintenance(v)}
                      disabled={maintenanceBusy()}
                      label="维护模式 · 开启后客户端 /api/* 返回 503"
                    />
                  </div>
                  <p class="text-xs text-content-tertiary tabular-nums font-mono">
                    <Show when={s().lastCheckedAt} fallback="最近检查:从未">
                      最近检查 · {new Date(s().lastCheckedAt!).toLocaleString('zh-CN')}
                    </Show>
                  </p>
                </div>
              </Card>

              {/* 主区域：稳定通道 */}
              <UpdateChannelCard
                channel="stable"
                status={s().stable}
                applying={applying() && pendingChannel() === 'stable'}
                onApply={() => openConfirm('stable')}
              />

              {/* 折叠区：Beta 通道；有新版本则在标题处亮 badge */}
              <Collapsible
                title="Beta 通道"
                badge={s().beta?.hasUpdate ? s().beta!.latestVersion : null}
              >
                <UpdateChannelCard
                  channel="beta"
                  status={s().beta}
                  applying={applying() && pendingChannel() === 'beta'}
                  onApply={() => openConfirm('beta')}
                />
              </Collapsible>

              {/* 升级进度（升级中或终态时显示） */}
              <Show when={progress() && (applying() || terminal())}>
                <Card>
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
                      class={`h-2 rounded-full transition-[width] duration-300 ease-out ${
                        terminal() === 'failed' ? 'bg-error' : 'bg-accent'
                      }`}
                      style={{ width: `${progress()!.percent}%` }}
                    />
                  </div>
                </Card>
              </Show>

              {/* 安全提示 */}
              <Card>
                <h3 class="text-sm font-semibold text-content mb-2">安全提示</h3>
                <ul class="text-xs text-content-tertiary space-y-1 list-disc pl-5">
                  <li>升级前会先 VACUUM INTO 备份当前数据库到 <code class="font-mono text-content">data/learning-{s().currentVersion}.backup.db</code></li>
                  <li>旧二进制会保留 2 份在安装目录（<code class="font-mono text-content">wordforge.{s().currentVersion}</code>）以便手动回滚</li>
                  <li>下载产物会校验 sha256；不匹配直接拒绝</li>
                  <li>跨通道升级允许（stable→beta 试用 / beta→stable 回归），但任一方向都需严格 semver 向上</li>
                  <li>当前裸跑模式下，进程通过 fork-exec 自重启；如果有 systemd / supervisor，请确保它们配置了 <code class="font-mono text-content">Restart=on-failure</code></li>
                </ul>
              </Card>
            </>
          )}
        </Show>
      </Show>

      <Modal
        open={confirmOpen()}
        onClose={() => {
          setConfirmOpen(false);
          setPendingChannel(null);
        }}
        title="确认一键更新"
      >
        <Show when={status() && pendingChannel() && currentChannelStatus()}>
          {(_) => (
            <div class="space-y-4">
              <p class="text-sm text-content-secondary">
                即将从 <span class="font-mono text-content">{status()!.currentVersion}</span> 升级到{' '}
                <span class="font-mono text-content font-semibold">
                  {currentChannelStatus()!.latestVersion}
                </span>
                （{CHANNEL_LABEL[pendingChannel()!]}）。
              </p>
              <ul class="text-sm text-content-secondary space-y-1 list-disc pl-5">
                <li>会先备份数据库</li>
                <li>替换二进制与前端 static 目录</li>
                <li>fork-exec 自重启（HTTP 连接会短暂中断）</li>
                <li>升级期间不要刷新页面或重启进程</li>
              </ul>
              <div class="flex justify-end gap-2 pt-2">
                <Button
                  variant="ghost"
                  onClick={() => {
                    setConfirmOpen(false);
                    setPendingChannel(null);
                  }}
                >
                  取消
                </Button>
                <Button onClick={confirmApply}>开始升级</Button>
              </div>
            </div>
          )}
        </Show>
      </Modal>

      {/* 回滚到上一版二次确认 */}
      <Modal
        open={rollbackOpen()}
        onClose={() => setRollbackOpen(false)}
        title="确认回滚到上一版"
      >
        <Show when={previousVersion() && status()}>
          <div class="space-y-3">
            <p class="text-sm text-content-secondary">
              将从 <span class="font-mono text-content">{status()!.currentVersion}</span> 切换到{' '}
              <span class="font-mono text-content font-semibold">{previousVersion()}</span>。
            </p>
            <p class="text-xs text-warning">
              ⚠ 后端策略要求版本只能向上迁移,因此回滚很可能会被后端拒绝。
              如确实需要降级,建议 SSH 登录服务器手动 swap 二进制 + 恢复对应 backup DB。
              本按钮仅用于"尝试自动回滚",失败时会显示后端原因。
            </p>
            <div class="flex justify-end gap-2 pt-2">
              <Button variant="ghost" onClick={() => setRollbackOpen(false)}>取消</Button>
              <Button onClick={confirmRollback}>尝试回滚</Button>
            </div>
          </div>
        </Show>
      </Modal>

      {/* S5：升级历史 */}
      <Collapsible title="升级历史">
        <Show when={!history.loading} fallback={<Spinner />}>
          <Show
            when={history()?.entries?.length}
            fallback={<p class="text-sm text-content-secondary py-2">暂无升级记录</p>}
          >
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
                <thead>
                  <tr class="text-left text-content-secondary border-b border-border">
                    <th class="pb-2 pr-4 font-medium">时间</th>
                    <th class="pb-2 pr-4 font-medium">版本</th>
                    <th class="pb-2 pr-4 font-medium">通道</th>
                    <th class="pb-2 font-medium">结果</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={history()!.entries}>
                    {(entry) => (
                      <tr class="border-b border-border/50 last:border-0">
                        <td class="py-2 pr-4 text-content-secondary font-mono text-xs">
                          {new Date(entry.startedAt).toLocaleString('zh-CN')}
                        </td>
                        <td class="py-2 pr-4 font-mono">
                          <span class="text-content-secondary">{entry.fromVersion}</span>
                          <span class="mx-1 text-content-tertiary">→</span>
                          <span class="text-content">{entry.toVersion}</span>
                        </td>
                        <td class="py-2 pr-4 text-content-secondary">{entry.channel}</td>
                        <td class="py-2">
                          <Badge
                            variant={
                              entry.outcome === 'success' ? 'success'
                              : entry.outcome === 'failed' ? 'error'
                              : 'default'
                            }
                            size="sm"
                          >
                            {OUTCOME_LABEL[entry.outcome] ?? entry.outcome}
                          </Badge>
                          <Show when={entry.error}>
                            <span class="ml-2 text-xs text-content-tertiary">{entry.error}</span>
                          </Show>
                        </td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </Show>
        </Show>
      </Collapsible>

      {/* anyHasUpdate 当前不展示在 UI，但保留 helper 给将来 dashboard badge 用 */}
      <Show when={false}>{anyHasUpdate(status())}{refetchHistory}</Show>
    </div>
  );
}
