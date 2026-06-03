import { createMemo, createResource, createSignal, Show, For, onCleanup, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { Spinner } from '@/components/ui/Spinner';
import { Switch } from '@/components/ui/Switch';
import { Collapsible } from '@/components/ui/Collapsible';
import { UpdateChannelCard } from '@/components/admin/UpdateChannelCard';
import { ReleaseNotesMarkdown } from '@/components/admin/ReleaseNotesMarkdown';
import { adminApi } from '@/api/admin';
import { ApiError, connectSseStream } from '@/api/http';
import { uiStore } from '@/stores/ui';
import { formatBytes, formatDateTime, formatDate } from '@/utils/formatters';
import type { AdminUpdateStatus, ChannelStatus, ChangelogSummary } from '@/types/admin';
import './updates.css';

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
  rolled_back: '已回滚',
};

// 流水线 6 步固定名 + 命中该步的 phase 集合（按设计稿顺序）
const PIPELINE_STEPS: Array<{ name: string; phases: string[]; eta: string }> = [
  { name: 'SQLite 备份', phases: ['backing_up_db'], eta: '~12s' },
  { name: '下载 tarball', phases: ['downloading', 'verifying', 'extracting'], eta: '~30s' },
  { name: '原子替换', phases: ['swapping'], eta: '~5s' },
  { name: '重启 systemd', phases: ['restarting'], eta: '~15s' },
  { name: '健康监测', phases: [], eta: '30s' },
  { name: '完成 · 退出维护', phases: ['completed'], eta: '~2s' },
];

// 把中文 phase 文案反查回原始 phase key（progress.phase 存的是 PHASE_LABEL 值）
const LABEL_TO_PHASE: Record<string, string> = Object.fromEntries(
  Object.entries(PHASE_LABEL).map(([k, v]) => [v, k]),
);

const CHANGELOG_GROUPS: Array<{ key: string; title: string }> = [
  { key: 'feat', title: 'Features' },
  { key: 'fix', title: 'Fixes' },
  { key: 'perf', title: 'Performance' },
  { key: 'docs', title: 'Docs' },
  { key: 'other', title: 'Other' },
];

// SHA-256 截断为 xxxx…xxxx
function shortSha(sha: string | null | undefined): string {
  if (!sha) return '—';
  return sha.length <= 12 ? sha : `${sha.slice(0, 4)}…${sha.slice(-4)}`;
}

// uptimeSecs → "Nd Nh"
function formatUptime(secs: number | null | undefined): string {
  if (secs == null || !isFinite(secs) || secs < 0) return '—';
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  if (d > 0) return `${d}d ${h}h`;
  const m = Math.floor((secs % 3600) / 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

const HEALTH_LABEL: Record<string, string> = { healthy: '健康', degraded: '降级', down: '宕机' };

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
  // 版本卡健康徽标
  const [health] = createResource(() => adminApi.getHealth());
  // 备份列表
  const [backups, { refetch: refetchBackups }] = createResource(() => adminApi.updatesBackups());
  // CHANGELOG：依赖 status，仅在任一通道 hasUpdate 时拉取
  const [changelog] = createResource<ChangelogSummary | null, AdminUpdateStatus | undefined>(
    () => status(),
    (s) => (anyHasUpdate(s) ? adminApi.updatesChangelog() : Promise.resolve(null)),
  );

  const [maintenanceBusy, setMaintenanceBusy] = createSignal(false);
  const [checking, setChecking] = createSignal(false);
  const [applying, setApplying] = createSignal(false);
  const [pendingChannel, setPendingChannel] = createSignal<'stable' | 'beta' | null>(null);
  const [confirmOpen, setConfirmOpen] = createSignal(false);
  // 回滚二次确认；rollbackTarget 为 null 时回退到 previousVersion()
  const [rollbackOpen, setRollbackOpen] = createSignal(false);
  const [rollbackBusy, setRollbackBusy] = createSignal(false);
  const [rollbackTarget, setRollbackTarget] = createSignal<string | null>(null);
  // 备份恢复二次确认 + 立即备份 busy + dry-run 预览
  const [restoreName, setRestoreName] = createSignal<string | null>(null);
  const [restoreBusy, setRestoreBusy] = createSignal(false);
  const [backupBusy, setBackupBusy] = createSignal(false);
  const [previewOpen, setPreviewOpen] = createSignal(false);
  const [progress, setProgress] = createSignal<{ phase: string; percent: number } | null>(null);
  // 终态标记：completed/failed 后 progress 保留显示(红/绿),不再被轮询清空
  const [terminal, setTerminal] = createSignal<'success' | 'failed' | null>(null);
  // SSE 最近一次推送时间,用于判断是否进入静默
  let lastSseAt = 0;

  /** 当前部署是否还应该提示「有更新」（任一通道 hasUpdate=true 即有） */
  function anyHasUpdate(s: AdminUpdateStatus | undefined): boolean {
    return !!(s?.stable?.hasUpdate || s?.beta?.hasUpdate);
  }

  // 升级目标通道：优先 stable 有更新，否则 beta
  const upgradeChannel = createMemo<'stable' | 'beta' | null>(() => {
    const s = status();
    if (s?.stable?.hasUpdate && s.stable.canApply) return 'stable';
    if (s?.beta?.hasUpdate && s.beta.canApply) return 'beta';
    if (s?.stable?.hasUpdate) return 'stable';
    if (s?.beta?.hasUpdate) return 'beta';
    return null;
  });
  const upgradeTarget = createMemo<ChannelStatus | null>(() => {
    const ch = upgradeChannel();
    const s = status();
    if (!ch || !s) return null;
    return ch === 'stable' ? s.stable : s.beta;
  });

  // 从 history 推导上一版:最新 success 的 fromVersion(必须不等于当前版本)
  const previousEntry = createMemo(() => {
    const entries = history()?.entries ?? [];
    const current = status()?.currentVersion;
    if (!current) return null;
    return (
      entries.find((e) => e.outcome === 'success' && e.fromVersion && e.fromVersion !== current) ??
      null
    );
  });
  const previousVersion = createMemo<string | null>(() => previousEntry()?.fromVersion ?? null);

  // 解析回滚目标版本的来源通道:优先取"曾安装该版本(toVersion===target)"那条 audit 的 channel,
  // 否则回退 previousEntry 的 channel,再否则 stable。避免审计/历史里通道恒记为 stable 与实际不符。
  const resolveRollbackChannel = (target: string): 'stable' | 'beta' => {
    const entries = history()?.entries ?? [];
    const e = entries.find((x) => x.toVersion === target) ?? previousEntry();
    return e?.channel === 'beta' ? 'beta' : 'stable';
  };

  // history 成功率:历史总数 vs success 数
  const historyStats = createMemo(() => {
    const entries = history()?.entries ?? [];
    if (entries.length === 0) return { total: 0, success: 0, rate: 0 };
    const success = entries.filter((e) => e.outcome === 'success').length;
    return { total: entries.length, success, rate: Math.round((success / entries.length) * 100) };
  });

  // 最近一次升级（history 第一条）的起始时间 + 耗时
  const lastRun = createMemo(() => {
    const e = history()?.entries?.[0];
    if (!e) return null;
    let durSecs: number | null = null;
    if (e.completedAt) {
      const d = (new Date(e.completedAt).getTime() - new Date(e.startedAt).getTime()) / 1000;
      if (isFinite(d) && d >= 0) durSecs = Math.round(d);
    }
    return { startedAt: e.startedAt, durSecs };
  });

  // 流水线步骤状态：done / running / pending
  const pipelineState = createMemo<Array<'done' | 'running' | 'pending'>>(() => {
    const p = progress();
    if (!applying() && !terminal()) return PIPELINE_STEPS.map(() => 'pending');
    const phase = p ? (LABEL_TO_PHASE[p.phase] ?? p.phase) : 'pending';
    const activeIdx = PIPELINE_STEPS.findIndex((st) => st.phases.includes(phase));
    if (terminal() === 'success' || phase === 'completed') {
      return PIPELINE_STEPS.map(() => 'done');
    }
    if (terminal() === 'failed' || phase === 'failed') {
      // 失败：把已知活跃步前的标 done，活跃步标 running（红由 progress 卡承载），其余 pending
      const idx = activeIdx < 0 ? 0 : activeIdx;
      return PIPELINE_STEPS.map((_, i) => (i < idx ? 'done' : i === idx ? 'running' : 'pending'));
    }
    if (activeIdx < 0) return PIPELINE_STEPS.map((_, i) => (i === 0 ? 'running' : 'pending'));
    return PIPELINE_STEPS.map((_, i) => (i < activeIdx ? 'done' : i === activeIdx ? 'running' : 'pending'));
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

  function openRollback(target?: string | null) {
    setRollbackTarget(target ?? null);
    setRollbackOpen(true);
  }

  async function confirmRollback() {
    const target = rollbackTarget() ?? previousVersion();
    const s = status();
    if (!target || !s) return;
    setRollbackOpen(false);
    setRollbackBusy(true);
    try {
      // m022:走 /rollback 专用端点,后端 allow_downgrade=true 跳过 semver 上升校验
      // 同时按 target tag 拉 GitHub release metadata 注入 updater cache 再 apply
      await adminApi.updatesRollback(resolveRollbackChannel(target), target, s.currentVersion);
      uiStore.toast.success(`已下发回滚到 ${target},等待重启...`);
      void refetchHistory();
      setTimeout(() => window.location.reload(), 2000);
    } catch (e) {
      const msg = e instanceof Error ? e.message : '未知错误';
      uiStore.toast.error('回滚失败', msg);
    } finally {
      setRollbackBusy(false);
    }
  }

  async function handleCreateBackup() {
    setBackupBusy(true);
    try {
      await adminApi.updatesCreateBackup();
      uiStore.toast.success('已创建手动备份');
      await refetchBackups();
    } catch (e) {
      uiStore.toast.error('备份失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBackupBusy(false);
    }
  }

  async function confirmRestore() {
    const name = restoreName();
    if (!name) return;
    setRestoreBusy(true);
    try {
      const res = await adminApi.updatesRestoreBackup(name);
      setRestoreName(null);
      uiStore.toast.success(
        '恢复完成',
        res.restartRecommended ? '建议重启后端进程使所有连接重读数据库' : undefined,
      );
      await Promise.all([refetch(), refetchBackups()]);
    } catch (e) {
      const msg = e instanceof Error ? e.message : '未知错误';
      uiStore.toast.error('恢复失败', msg);
    } finally {
      setRestoreBusy(false);
    }
  }

  async function downloadBackup(name: string) {
    try {
      const url = await adminApi.updatesBackupDownloadUrl(name);
      const a = document.createElement('a');
      a.href = url;
      a.download = name;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 5000);
    } catch (e) {
      uiStore.toast.error('下载失败', e instanceof Error ? e.message : '未知错误');
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

  const healthVariant = (st: string | undefined) =>
    st === 'healthy' ? 'success' : st === 'degraded' ? 'warning' : 'error';

  return (
    <div class="space-y-4">
      <Show when={!status.loading} fallback={<Spinner />}>
        <Show when={status()}>
          {(s) => (
            <>
              {/* 1. Hero —— 新版本就绪 / 当前最新 + 版本卡 */}
              <section class="update-hero">
                <div class="hero-grid">
                  <div>
                    <p class="text-caption" style={{ color: 'var(--accent)' }}>
                      {anyHasUpdate(s()) ? '有可用更新' : '当前最新'}
                    </p>
                    <Show
                      when={upgradeTarget()}
                      fallback={
                        <h1 class="lh1">当前已是最新版本 <em>{s().currentVersion}</em></h1>
                      }
                    >
                      <h1 class="lh1">新版本 <em>{upgradeTarget()!.latestVersion}</em> 已就绪，可一键升级</h1>
                    </Show>
                    <p class="lh-sub">
                      一键升级走 GitHub Release：先 VACUUM INTO 备份 SQLite、下载 tarball、sha256 校验、原子替换二进制、fork-exec 自重启。维护模式开启时客户端 /api/* 全部返回 503。
                      <Show when={changelog()?.available && changelog()!.totalCommits != null}>
                        {' '}本次包含 {changelog()!.totalCommits} commits。
                      </Show>
                    </p>
                    <div class="actions">
                      <Show when={upgradeChannel()}>
                        <Button
                          size="lg"
                          onClick={() => openConfirm(upgradeChannel()!)}
                          disabled={!upgradeTarget()?.canApply}
                          loading={applying()}
                        >
                          立即升级到 {upgradeTarget()!.latestVersion}
                        </Button>
                      </Show>
                      <Button
                        variant="secondary"
                        size="lg"
                        onClick={() => {
                          const el = document.getElementById('changelog-section');
                          el?.scrollIntoView({ behavior: 'smooth' });
                        }}
                      >
                        查看完整 CHANGELOG
                      </Button>
                      <Show when={upgradeTarget()}>
                        <Button variant="ghost" size="lg" onClick={() => setPreviewOpen(true)}>
                          预览升级（dry-run）
                        </Button>
                      </Show>
                      <Button variant="ghost" size="lg" onClick={handleCheck} loading={checking()}>
                        立即检查
                      </Button>
                    </div>
                  </div>

                  <div class="version-card">
                    <div class="row-top">
                      <div>
                        <span class="text-caption">当前版本</span>
                        <div class="v">{s().currentVersion}</div>
                        <div class="delta">
                          <Show when={s().installedAt} fallback="安装时间未知">
                            {formatDate(s().installedAt!)} 安装
                          </Show>
                          {' · 已运行 '}{formatUptime(s().uptimeSecs)}
                        </div>
                      </div>
                      <Badge variant={healthVariant(health()?.status)} dot size="sm">
                        {HEALTH_LABEL[health()?.status ?? ''] ?? '—'}
                      </Badge>
                    </div>
                    <div class="vs">
                      <div>
                        <div class="l">当前</div>
                        <div class="vv">{s().currentVersion}</div>
                      </div>
                      <span class="arrow" aria-hidden>
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M5 12h14M13 6l6 6-6 6" /></svg>
                      </span>
                      <div>
                        <div class="l" style={{ color: 'var(--accent)' }}>{upgradeTarget() ? '可升级' : '最新'}</div>
                        <div class="vv" style={{ color: 'var(--accent)' }}>
                          {upgradeTarget()?.latestVersion ?? s().currentVersion}
                        </div>
                      </div>
                    </div>
                    <div class="upd-divider" />
                    <Show when={upgradeTarget()}>
                      <div class="upd-row-spread text-small">
                        <span class="text-tertiary">tarball 大小</span>
                        <strong class="text-mono">{formatBytes(upgradeTarget()!.tarballSize)}</strong>
                      </div>
                      <div class="upd-row-spread text-small" style={{ 'margin-top': '4px' }}>
                        <span class="text-tertiary">SHA-256</span>
                        <strong class="text-mono">{shortSha(upgradeTarget()!.sha256)}</strong>
                      </div>
                    </Show>
                    <div class="upd-row-spread text-small" style={{ 'margin-top': '4px' }}>
                      <span class="text-tertiary">来源</span>
                      <Show when={upgradeTarget()?.releaseUrl} fallback={<span class="text-mono text-tertiary">—</span>}>
                        <a class="text-accent text-mono" href={upgradeTarget()!.releaseUrl} target="_blank" rel="noopener">GitHub →</a>
                      </Show>
                    </div>
                  </div>
                </div>
              </section>

              {/* 2. 维护模式（琥珀卡） */}
              <div class="maint-card">
                <div class="ic" aria-hidden>
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" /></svg>
                </div>
                <div class="info">
                  <strong>维护模式</strong>
                  <span>开启后所有客户端 /api/* 返回 503，仅放行 /admin/*。建议升级前 30s 开启。</span>
                </div>
                <span class="text-mono text-small text-tertiary" style={{ 'margin-right': '4px' }}>
                  当前 {settings()?.maintenanceMode ? 'OPEN' : 'CLOSED'}
                </span>
                <Switch
                  checked={!!settings()?.maintenanceMode}
                  onChange={(v) => toggleMaintenance(v)}
                  disabled={maintenanceBusy()}
                />
              </div>

              {/* 3. 升级流水线 6 步 */}
              <div class="flex items-baseline gap-3">
                <h2 class="text-base font-semibold text-content">升级流水线</h2>
                <span class="text-small text-tertiary">原子替换 + 失败回滚 + 健康监测 30s</span>
                <Show when={lastRun()}>
                  <span class="text-small text-mono text-tertiary ml-auto">
                    最近一次 {formatDateTime(lastRun()!.startedAt)}
                    <Show when={lastRun()!.durSecs != null}> · 耗时 {lastRun()!.durSecs}s</Show>
                  </span>
                </Show>
              </div>
              <Card>
                <div class="pipeline">
                  <For each={PIPELINE_STEPS}>
                    {(step, i) => {
                      const st = () => pipelineState()[i()];
                      return (
                        <div classList={{ step: true, 'is-done': st() === 'done', 'is-running': st() === 'running', 'is-pending': st() === 'pending' }}>
                          <div class="head">
                            <span class="ic" aria-hidden>
                              <Show
                                when={st() === 'done'}
                                fallback={
                                  <Show
                                    when={st() === 'running'}
                                    fallback={<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /></svg>}
                                  >
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 3" /></svg>
                                  </Show>
                                }
                              >
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12" /></svg>
                              </Show>
                            </span>
                            <div><div class="name">{step.name}</div></div>
                          </div>
                          <div class="status">
                            {st() === 'done' ? '完成 ✓' : st() === 'running' ? '运行中…' : '待运行'}
                          </div>
                          <div class="duration">
                            {st() === 'done' ? '已完成' : st() === 'running' ? '进行中…' : `预计 ${step.eta}`}
                          </div>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </Card>

              {/* 升级进度（升级中或终态时显示，保留原百分比进度条） */}
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

              {/* 4. 主网格：CHANGELOG (span 7) + 备份 (span 5) */}
              <div class="grid grid-cols-1 lg:grid-cols-12 gap-4">
                {/* CHANGELOG */}
                <div class="upd-panel lg:col-span-7" id="changelog-section">
                  <div class="upd-panel-header">
                    <h3>CHANGELOG{upgradeTarget() ? ` · ${upgradeTarget()!.latestVersion}` : ''}</h3>
                    <Show when={changelog()?.available}>
                      <span class="text-mono text-small text-tertiary">
                        {changelog()!.totalCommits ?? 0} commits · {changelog()!.contributors ?? 0} contributors
                      </span>
                      <div class="aside">
                        <For each={CHANGELOG_GROUPS}>
                          {(g) => (
                            <Show when={(changelog()!.categoryCounts?.[g.key] ?? 0) > 0}>
                              <span classList={{ 'upd-chip': true, 'is-success': g.key === 'feat', 'is-warning': g.key === 'fix', 'is-accent': g.key === 'perf' }}>
                                {changelog()!.categoryCounts![g.key]} {g.key}
                              </span>
                            </Show>
                          )}
                        </For>
                      </div>
                    </Show>
                  </div>
                  <div class="upd-panel-body">
                    {/* 完整更新日志：优先展示 release notes 富文本（覆盖全部变更，如 19 项 backlog），
                        compare commits 派生列表仅在无 release notes 时作回退（如 stable→stable 同 body 缺失场景） */}
                    <Show
                      when={upgradeTarget()?.releaseNotes}
                      fallback={
                        <Show
                          when={changelog()?.available}
                          fallback={<p class="text-small text-tertiary">暂无可比对的 CHANGELOG。</p>}
                        >
                          <div class="changelog">
                            <For each={CHANGELOG_GROUPS}>
                              {(g) => {
                                const items = () => (changelog()!.commits ?? []).filter((c) => c.category === g.key);
                                return (
                                  <Show when={items().length > 0}>
                                    <h4>{g.title}</h4>
                                    <ul>
                                      <For each={items()}>
                                        {(c) => (
                                          <li class={g.key}>
                                            <Show when={c.scope}><span class="scope">{c.scope}</span></Show>
                                            {c.subject} <code>{c.sha}</code>
                                          </li>
                                        )}
                                      </For>
                                    </ul>
                                  </Show>
                                );
                              }}
                            </For>
                          </div>
                        </Show>
                      }
                    >
                      <ReleaseNotesMarkdown content={upgradeTarget()!.releaseNotes} />
                    </Show>
                    <div class="upd-divider" />
                    <div class="upd-row-spread">
                      <Show when={changelog()?.compareUrl}>
                        <a class="text-accent text-small" href={changelog()!.compareUrl} target="_blank" rel="noopener">
                          本地比较 {changelog()!.base}...{changelog()!.head} →
                        </a>
                      </Show>
                      <Show when={upgradeTarget()?.releaseUrl}>
                        <a class="text-accent text-small ml-auto" href={upgradeTarget()!.releaseUrl} target="_blank" rel="noopener">
                          查看 GitHub release notes →
                        </a>
                      </Show>
                    </div>
                  </div>
                </div>

                {/* 备份 */}
                <div class="upd-panel lg:col-span-5">
                  <div class="upd-panel-header">
                    <h3>SQLite 备份</h3>
                    <span class="text-mono text-small text-tertiary">
                      最近 {backups()?.backups?.length ?? 0} 个 · 自动保留 30 个
                    </span>
                    <div class="aside">
                      <Button size="sm" variant="secondary" onClick={handleCreateBackup} loading={backupBusy()}>
                        立即备份
                      </Button>
                    </div>
                  </div>
                  <div class="upd-panel-body">
                    <Show
                      when={backups()?.backups?.length}
                      fallback={<p class="text-small text-tertiary">暂无备份。</p>}
                    >
                      <For each={backups()!.backups}>
                        {(b) => (
                          <div class="backup-row">
                            <span class="ic" aria-hidden>
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><ellipse cx="12" cy="5" rx="9" ry="3" /><path d="M3 5v14a9 3 0 0 0 18 0V5" /><path d="M3 12a9 3 0 0 0 18 0" /></svg>
                            </span>
                            <div class="name">
                              <strong>{b.name}</strong>
                              <span>{BACKUP_KIND_LABEL[b.kind] ?? b.kind}{b.version ? ` · ${b.version}` : ''}</span>
                            </div>
                            <span class="size">{formatBytes(b.sizeBytes)}</span>
                            <span class="date">{formatDateTime(b.createdAt)}</span>
                            <div class="actions">
                              <Button size="xs" variant="ghost" onClick={() => downloadBackup(b.name)} title="下载">下载</Button>
                              <Button size="xs" variant="warning" onClick={() => setRestoreName(b.name)}>恢复…</Button>
                            </div>
                          </div>
                        )}
                      </For>
                    </Show>
                    <div class="upd-divider" />
                    <div class="upd-row-spread text-small">
                      <span class="text-tertiary">
                        总占用 {formatBytes(backups()?.totalBytes ?? 0)} · 阈值 {formatBytes(backups()?.thresholdBytes ?? 0)}
                      </span>
                    </div>
                  </div>
                </div>
              </div>

              {/* 5. 版本历史时间线 */}
              <div class="flex items-baseline gap-3">
                <h2 class="text-base font-semibold text-content">版本历史</h2>
                <span class="text-small text-tertiary">历史成功率 {history() ? `${historyStats().rate}%` : '—'} · 一键回滚走 /rollback 端点</span>
              </div>
              <Card>
                <Show when={!history.loading} fallback={<Spinner />}>
                  <Show
                    when={history()?.entries?.length}
                    fallback={<p class="text-small text-tertiary py-2">暂无升级记录</p>}
                  >
                    <div class="timeline">
                      <For each={history()!.entries}>
                        {(entry) => {
                          const isCurrent = () => entry.toVersion === s().currentVersion && entry.outcome === 'success';
                          const isRollback = () => entry.outcome === 'failed' || entry.outcome === 'rolled_back';
                          return (
                            <div classList={{ 'timeline-item': true, 'is-current': isCurrent(), 'is-rollback': isRollback() }}>
                              <span class="dot" aria-hidden />
                              <div class="lv">
                                <span>{entry.fromVersion} → {entry.toVersion}</span>
                                <Show when={isCurrent()}>
                                  <span class="label feat">当前运行</span>
                                </Show>
                                <Badge
                                  variant={
                                    entry.outcome === 'success' ? 'success'
                                    : entry.outcome === 'failed' ? 'error'
                                    : entry.outcome === 'rolled_back' ? 'warning'
                                    : 'default'
                                  }
                                  size="sm"
                                >
                                  {OUTCOME_LABEL[entry.outcome] ?? entry.outcome}
                                </Badge>
                                <span class="date">{entry.channel} · {formatDateTime(entry.startedAt)}</span>
                              </div>
                              <Show when={entry.error}>
                                <div class="desc text-error">{entry.error}</div>
                              </Show>
                              <Show when={!isCurrent() && entry.fromVersion && entry.fromVersion !== s().currentVersion}>
                                <div class="actions">
                                  <Button size="xs" variant="warning" onClick={() => openRollback(entry.fromVersion)}>
                                    回滚到 {entry.fromVersion}…
                                  </Button>
                                </div>
                              </Show>
                            </div>
                          );
                        }}
                      </For>
                    </div>
                  </Show>
                </Show>
              </Card>

              {/* 通道卡（保留既有 apply/canApply 语义；Stable 主区 + Beta 折叠区） */}
              <Collapsible title="通道详情 · 稳定通道" defaultOpen={true}>
                <UpdateChannelCard
                  channel="stable"
                  status={s().stable}
                  applying={applying() && pendingChannel() === 'stable'}
                  onApply={() => openConfirm('stable')}
                />
              </Collapsible>
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

              {/* 安全提示 */}
              <Card>
                <h3 class="text-sm font-semibold text-content mb-2">安全提示</h3>
                <ul class="text-xs text-content-tertiary space-y-1 list-disc pl-5">
                  <li>升级前会先 VACUUM INTO 备份当前数据库到 <code class="font-mono text-content">data/learning-{s().currentVersion}.backup.db</code></li>
                  <li>旧二进制会保留 2 份在安装目录（<code class="font-mono text-content">wordforge.{s().currentVersion}</code>）以便手动回滚</li>
                  <li>下载产物会校验 sha256；不匹配直接拒绝</li>
                  <li>跨通道升级允许（stable→beta 试用 / beta→stable 回归），但任一方向都需严格 semver 向上</li>
                  <li>备份恢复会先做 pre-restore 兜底；在线页拷贝后建议重启进程让连接池重读</li>
                </ul>
              </Card>
            </>
          )}
        </Show>
      </Show>

      {/* 确认一键更新 */}
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

      {/* 回滚二次确认 */}
      <Modal
        open={rollbackOpen()}
        onClose={() => setRollbackOpen(false)}
        title="确认回滚版本"
      >
        <Show when={(rollbackTarget() ?? previousVersion()) && status()}>
          <div class="space-y-3">
            <p class="text-sm text-content-secondary">
              将从 <span class="font-mono text-content">{status()!.currentVersion}</span> 切换到{' '}
              <span class="font-mono text-content font-semibold">{rollbackTarget() ?? previousVersion()}</span>。
            </p>
            <p class="text-xs text-warning">
              ⚠ 此操作走 <code class="font-mono">/admin/updates/rollback</code> 端点,
              后端会跳过 semver 上升校验、从 GitHub 拉 target tag 的 release metadata 再走标准 apply 流程
              (sha256 校验 / DB 备份 / fork-exec 自重启)。如目标 tag 在 GitHub 已删除会失败。
            </p>
            <div class="flex justify-end gap-2 pt-2">
              <Button variant="ghost" onClick={() => setRollbackOpen(false)}>取消</Button>
              <Button variant="warning" onClick={confirmRollback} loading={rollbackBusy()}>尝试回滚</Button>
            </div>
          </div>
        </Show>
      </Modal>

      {/* 备份恢复二次确认（强警示） */}
      <Modal
        open={!!restoreName()}
        onClose={() => setRestoreName(null)}
        title="确认从备份恢复"
        closeOnBackdrop={false}
      >
        <div class="space-y-3">
          <p class="text-sm text-content-secondary">
            将用备份 <span class="font-mono text-content">{restoreName()}</span> 覆盖当前数据库。
          </p>
          <p class="text-xs text-warning">
            ⚠ 此操作会用该备份覆盖当前数据库。系统已自动做好 pre-restore 兜底备份；
            恢复为在线页拷贝，完成后<strong>建议重启后端进程</strong>使连接池重读数据。此操作不可中途取消。
          </p>
          <div class="flex justify-end gap-2 pt-2">
            <Button variant="ghost" onClick={() => setRestoreName(null)}>取消</Button>
            <Button variant="danger" onClick={confirmRestore} loading={restoreBusy()}>确认恢复</Button>
          </div>
        </div>
      </Modal>

      {/* dry-run 预览（只展示流水线 + changelog，不执行） */}
      <Modal
        open={previewOpen()}
        onClose={() => setPreviewOpen(false)}
        title="升级预览（dry-run）"
        size="lg"
      >
        <div class="space-y-4">
          <p class="text-sm text-content-secondary">
            以下为升级 {upgradeTarget()?.latestVersion ?? ''} 将执行的步骤，仅预览，不会真正升级。
          </p>
          <ol class="text-sm text-content-secondary space-y-1 list-decimal pl-5">
            <For each={PIPELINE_STEPS}>{(st) => <li>{st.name}</li>}</For>
          </ol>
          <Show when={changelog()?.available}>
            <div class="upd-divider" />
            <p class="text-sm text-content">
              本次包含 {changelog()!.totalCommits ?? 0} commits · {changelog()!.contributors ?? 0} contributors。
            </p>
          </Show>
          <div class="flex justify-end gap-2 pt-2">
            <Button variant="ghost" onClick={() => setPreviewOpen(false)}>关闭</Button>
            <Show when={upgradeChannel()}>
              <Button
                onClick={() => {
                  setPreviewOpen(false);
                  openConfirm(upgradeChannel()!);
                }}
                disabled={!upgradeTarget()?.canApply}
              >
                确认升级
              </Button>
            </Show>
          </div>
        </div>
      </Modal>
    </div>
  );
}

const BACKUP_KIND_LABEL: Record<string, string> = {
  upgrade: '升级前',
  daily: '每日定时',
  manual: '手动',
  pre_restore: '恢复前兜底',
};
