import { createEffect, createMemo, createResource, createSignal, For, Show, onMount, onCleanup, startTransition, type JSX } from 'solid-js';
import {
  PageHead, Panel, Btn, IconBtn, Badge, Icon, Field, Seg, Switch, Confirm, Loading, BarChart,
  fmtNum, fmtBytes, fmtAgo, fmtTime, sx, toast,
} from '@/components/wf';
import { adminApi } from '@/api/admin';
import { ApiError } from '@/api/http';
import type {
  AdminUpdateStatus, ChannelStatus, ChangelogSummary, BackupList, BackupEntry, UpdateAuditEntry,
} from '@/types/admin';

type Channel = 'stable' | 'beta';

// 后端 apply task 的 phase 标识 → 中文短句（升级进度条文案）
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

// CHANGELOG 分类展示元数据（标题 + 颜色），与设计稿 feat/fix/perf 对齐
const CHANGELOG_META: Array<{ key: string; label: string; color: string }> = [
  { key: 'feat', label: '新功能', color: 'var(--success)' },
  { key: 'fix', label: '修复', color: 'var(--info)' },
  { key: 'perf', label: '性能', color: 'var(--accent)' },
];

// release notes(GitHub Release body / CHANGELOG.md 段落）markdown → 结构化分节。
// 后端无 GitHub compare 解析的 commits（changelogGroups 为空）时，把 releaseNotes 的
// markdown 解析成「分节标题 + 列表」对齐设计稿，避免把 markdown 源码原样 dump。
interface ReleaseNoteSection { label: string; color: string; items: string[]; }
function stripInlineMd(s: string): string {
  return s
    .replace(/\*\*(.+?)\*\*/g, '$1')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\[(.+?)\]\([^)]*\)/g, '$1')
    .trim();
}
function releaseNoteColor(label: string): string {
  if (/新功能|新增|feat|feature/i.test(label)) return 'var(--success)';
  if (/修复|fix|bug/i.test(label)) return 'var(--info)';
  if (/性能|优化|perf/i.test(label)) return 'var(--accent)';
  return 'var(--text-2)';
}
function parseReleaseNotes(md: string): { intro: string; sections: ReleaseNoteSection[] } {
  const sections: ReleaseNoteSection[] = [];
  let intro = '';
  let cur: ReleaseNoteSection | null = null;
  for (const raw of md.split('\n')) {
    const line = raw.trim();
    if (!line) continue;
    if (line === '---' || /^\*\*Full Changelog\*\*/i.test(line)) break; // 跳过页脚（分隔线 / compare 链接）
    const h = /^#{1,6}\s+(.*)$/.exec(line);
    if (h) {
      cur = { label: stripInlineMd(h[1]), color: releaseNoteColor(h[1]), items: [] };
      sections.push(cur);
      continue;
    }
    const b = /^[-*]\s+(.*)$/.exec(line);
    if (b) {
      if (cur) cur.items.push(stripInlineMd(b[1]));
      continue;
    }
    if (!cur) intro = intro ? `${intro} ${stripInlineMd(line)}` : stripInlineMd(line); // 首个标题前的段落作引言
  }
  return { intro, sections: sections.filter((s) => s.items.length > 0) };
}

// 升级 / 回滚触发后端 fork-exec 自重启：轮询 /api/status 探活，服务回来再整页重载，
// 避免在重启窗口内硬重载撞 502（旧裸 setTimeout 重载的问题）。超时兜底仍重载。
function reloadWhenBack(maxWaitMs = 60_000) {
  const start = Date.now();
  const schedule = () => {
    if (Date.now() - start < maxWaitMs) setTimeout(tick, 1500);
    else window.location.reload();
  };
  const tick = () => {
    fetch('/api/status', { cache: 'no-store' })
      .then((r) => { if (r.ok) window.location.reload(); else schedule(); })
      .catch(schedule);
  };
  setTimeout(tick, 1200); // 给重启一个起步窗口
}

// 备份种类 → 中文短标
const BACKUP_KIND_LABEL: Record<string, string> = {
  upgrade: '升级',
  daily: '每日',
  manual: '手动',
  pre_restore: '恢复前',
};

// 升级进行中需要继续轮询的状态判定
function applyInFlight(s: AdminUpdateStatus | undefined): boolean {
  const t = s?.applyTask;
  return !!t && t.phase !== 'completed' && t.phase !== 'failed' && !t.error;
}

// SHA-256 截断展示
function shortSha(sha: string | null | undefined): string {
  if (!sha) return '—';
  return sha.length <= 16 ? sha : `${sha.slice(0, 16)}…`;
}

// 单条审计 → 耗时（秒）；缺 completedAt 返回 null
function auditDurSecs(e: UpdateAuditEntry): number | null {
  if (!e.completedAt) return null;
  const d = (new Date(e.completedAt).getTime() - new Date(e.startedAt).getTime()) / 1000;
  return isFinite(d) && d >= 0 ? Math.round(d) : null;
}

export default function UpdatesPage() {
  // status 用无 source 资源；apply 进行中靠 startPoll 定时 refetch 刷新
  const [status, { refetch }] = createResource(() => adminApi.updatesStatus());
  const [history, { refetch: refetchHistory }] = createResource(() => adminApi.updatesHistory());
  const [backups, { refetch: refetchBackups }] = createResource<BackupList>(() => adminApi.updatesBackups());
  const [settings, { refetch: refetchSettings }] = createResource(() => adminApi.getSettings());
  const [health] = createResource(() => adminApi.getHealth());
  // CHANGELOG 依赖所选通道；任一通道有更新时才拉取
  const [channel, setChannel] = createSignal<Channel>('stable');
  const [changelog] = createResource<ChangelogSummary | null, Channel>(
    channel,
    (ch) => adminApi.updatesChangelog(ch).catch(() => null),
  );

  const [busy, setBusy] = createSignal(false);
  const [maintBusy, setMaintBusy] = createSignal(false);
  // confirm: 'apply' | 'preview' | 'rollback' | {kind:'restore', backup}
  const [confirm, setConfirm] = createSignal<
    'apply' | 'preview' | 'rollback' | { kind: 'restore'; backup: BackupEntry } | null
  >(null);
  const [rollbackTarget, setRollbackTarget] = createSignal<string | null>(null);
  // 升级进度（apply 进行中 / 终态保留）
  const [progress, setProgress] = createSignal<{ phase: string; percent: number; failed?: boolean } | null>(null);

  // 当前所选通道的 ChannelStatus
  const chStatus = createMemo<ChannelStatus | null>(() => {
    const s = status();
    if (!s) return null;
    return channel() === 'stable' ? s.stable : s.beta;
  });
  const targetVersion = createMemo<string | null>(() => chStatus()?.latestVersion ?? null);

  // 从 history 推导上一可回滚版本：最新 success 且 fromVersion≠当前版本
  const previousEntry = createMemo<UpdateAuditEntry | null>(() => {
    const entries = history()?.entries ?? [];
    const current = status()?.currentVersion;
    if (!current) return null;
    return entries.find(
      (e) => e.outcome === 'success' && e.fromVersion && e.fromVersion !== current,
    ) ?? null;
  });

  // 推导回滚目标版本的来源通道：曾安装该版本那条审计的 channel，否则 previousEntry，否则 stable
  function resolveRollbackChannel(target: string): Channel {
    const entries = history()?.entries ?? [];
    const e = entries.find((x) => x.toVersion === target) ?? previousEntry();
    return e?.channel === 'beta' ? 'beta' : 'stable';
  }

  // CHANGELOG 按 category 聚合为 feat/fix/perf
  const changelogGroups = createMemo(() => {
    const cl = changelog();
    if (!cl?.available || !cl.commits?.length) return [];
    return CHANGELOG_META.map((m) => ({
      ...m,
      items: cl.commits!.filter((c) => c.category === m.key),
    })).filter((g) => g.items.length > 0);
  });

  // releaseNotes（GitHub Release body）markdown → 结构化分节，供 changelogGroups 为空时渲染
  const releaseNotesSections = createMemo(() => {
    const notes = chStatus()?.releaseNotes;
    return notes ? parseReleaseNotes(notes) : { intro: '', sections: [] as ReleaseNoteSection[] };
  });

  // 升级进行中时启动 2s 轮询，终态后停止
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  function stopPoll() {
    if (pollTimer) { clearInterval(pollTimer); pollTimer = undefined; }
  }
  function startPoll() {
    if (pollTimer) return;
    pollTimer = setInterval(() => void startTransition(() => refetch()), 2000);
  }

  // 监听 status 变化驱动 progress 卡 + 轮询生命周期（createEffect eager 执行，副作用必须用它而非 createMemo）
  createEffect(() => {
    const s = status();
    const t = s?.applyTask;
    if (!t) return;
    const failed = t.phase === 'failed' || !!t.error;
    const done = t.phase === 'completed';
    setProgress({ phase: PHASE_LABEL[t.phase] ?? t.phase, percent: t.percent, failed });
    if (failed || done) {
      stopPoll();
      if (done) {
        toast.success('升级成功', '服务重启完成后自动刷新…');
        reloadWhenBack();
      } else {
        toast.error('升级失败', t.error ?? '后端 apply task 报错');
      }
    } else if (applyInFlight(s)) {
      startPoll();
    }
  });
  onMount(() => { if (applyInFlight(status())) startPoll(); });
  onCleanup(stopPoll);

  async function doCheck() {
    setBusy(true);
    try {
      const fresh = await adminApi.updatesCheck();
      const tags: string[] = [];
      if (fresh.stable?.hasUpdate) tags.push(`stable ${fresh.stable.latestVersion}`);
      if (fresh.beta?.hasUpdate) tags.push(`beta ${fresh.beta.latestVersion}`);
      if (tags.length) toast.success('发现新版本', tags.join('、'));
      else toast.info('当前已是最新版本', fresh.currentVersion);
      await refetch();
    } catch (e) {
      toast.error('检查失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy(false);
    }
  }

  async function doApply() {
    const s = status();
    const target = targetVersion();
    if (!s || !target) return;
    setBusy(true);
    setProgress({ phase: PHASE_LABEL.pending, percent: 0 });
    try {
      await adminApi.updatesApply(channel(), target, s.currentVersion);
      setConfirm(null);
      toast.success('已发起升级', '后台异步执行，下方流水线将实时刷新进度');
      startPoll();
      await Promise.all([refetch(), refetchHistory()]);
    } catch (e) {
      setProgress(null);
      const msg = e instanceof ApiError ? `[${e.code}] ${e.message}` : e instanceof Error ? e.message : '未知错误';
      toast.error('升级启动失败', msg);
    } finally {
      setBusy(false);
    }
  }

  async function doRollback() {
    const s = status();
    const target = rollbackTarget() ?? previousEntry()?.fromVersion ?? null;
    if (!s || !target) return;
    setBusy(true);
    try {
      await adminApi.updatesRollback(resolveRollbackChannel(target), target, s.currentVersion);
      setConfirm(null);
      toast.success(`已下发回滚到 ${target}`, '服务重启完成后自动刷新…');
      void refetchHistory();
      reloadWhenBack();
    } catch (e) {
      toast.error('回滚失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy(false);
    }
  }

  async function createBackup() {
    setBusy(true);
    try {
      await adminApi.updatesCreateBackup();
      toast.success('已创建手动备份');
      await refetchBackups();
    } catch (e) {
      toast.error('备份失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy(false);
    }
  }

  async function restoreBackup(b: BackupEntry) {
    setBusy(true);
    try {
      const res = await adminApi.updatesRestoreBackup(b.name);
      setConfirm(null);
      toast.success('已从备份恢复', res.restartRecommended ? '建议重启后端进程使连接池重读数据库' : undefined);
      await Promise.all([refetch(), refetchBackups()]);
    } catch (e) {
      toast.error('恢复失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setBusy(false);
    }
  }

  async function downloadBackup(b: BackupEntry) {
    try {
      const url = await adminApi.updatesBackupDownloadUrl(b.name);
      const a = document.createElement('a');
      a.href = url;
      a.download = b.name;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 5000);
    } catch (e) {
      toast.error('下载失败', e instanceof Error ? e.message : '未知错误');
    }
  }

  async function toggleMaintenance(v: boolean) {
    setMaintBusy(true);
    try {
      await adminApi.setMaintenance(v);
      await refetchSettings();
      v ? toast.warning('维护模式已开启') : toast.success('维护模式已关闭');
    } catch (e) {
      toast.error('维护模式切换失败', e instanceof Error ? e.message : '未知错误');
    } finally {
      setMaintBusy(false);
    }
  }

  const healthMeta = (st: string | undefined): { variant: 'success' | 'warning' | 'error'; label: string } =>
    st === 'healthy' ? { variant: 'success', label: '健康' }
      : st === 'degraded' ? { variant: 'warning', label: '降级' }
        : { variant: 'error', label: st ? '宕机' : '—' };

  return (
    <div>
      <PageHead
        title="版本更新"
        desc="一键自更新流水线：检查、灰度升级、回滚，含 CHANGELOG 与 SQLite 备份恢复。"
        right={
          <>
            <Show when={health()}>
              {(h) => (
                <Badge variant={healthMeta(h().status).variant} dot>{healthMeta(h().status).label}</Badge>
              )}
            </Show>
            <Btn variant="secondary" icon="refresh" onClick={doCheck} disabled={busy()}>检查更新</Btn>
          </>
        }
      />

      {/* 升级流水线 + CHANGELOG */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1fr) minmax(0,1.2fr)', gap: 16, marginBottom: 16 })}
      >
        <Panel title="升级流水线">
          <Show when={status()} fallback={<Loading />}>
            {(s) => (
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
                <div style={sx({ display: 'flex', alignItems: 'center', gap: 14, padding: 14, borderRadius: 12, background: 'var(--surface-sunken)' })}>
                  <div style={sx({ textAlign: 'center' })}>
                    <div class="muted-3" style={sx({ fontSize: 11 })}>当前</div>
                    <div class="mono" style={sx({ fontSize: 19, fontWeight: 800 })}>{s().currentVersion}</div>
                  </div>
                  <Icon name="chevR" size={20} style={sx({ color: 'var(--text-3)' })} />
                  <div style={sx({ textAlign: 'center' })}>
                    <div class="muted-3" style={sx({ fontSize: 11 })}>最新 {channel()}</div>
                    <div class="mono" style={sx({ fontSize: 19, fontWeight: 800, color: 'var(--accent)' })}>
                      {targetVersion() ?? s().currentVersion}
                    </div>
                  </div>
                  <Show when={chStatus()?.hasUpdate}>
                    <Badge variant="accent" style={sx({ marginLeft: 'auto' })}>可升级</Badge>
                  </Show>
                </div>

                <Field label="升级通道">
                  <Seg
                    options={[{ value: 'stable', label: '稳定通道' }, { value: 'beta', label: 'Beta 通道' }]}
                    value={channel()}
                    onChange={(v) => setChannel(v as Channel)}
                  />
                </Field>

                {/* 当前版本元信息 */}
                <div style={sx({ display: 'flex', flexWrap: 'wrap', gap: 8, fontSize: 11.5 })}>
                  <span class="muted-3">
                    {/* 当前只有一个二进制，它只属于一个通道（版本含 '-' 预发布标记 → beta，否则 stable）；
                        「安装于」是该二进制的安装时间，仅在所选通道与其所属通道一致时才显示，
                        避免在另一通道下误显示成「该通道已安装于同一时间」。 */}
                    <Show
                      when={channel() === ((s().currentVersion || '').includes('-') ? 'beta' : 'stable')}
                      fallback={<>当前未安装此通道版本</>}
                    >
                      安装于 {s().installedAt ? fmtTime(s().installedAt!).slice(0, 10) : '未知'}
                    </Show>
                    {' · '}tarball{' '}
                    <span class="mono">{chStatus() ? fmtBytes(chStatus()!.tarballSize) : '—'}</span> · SHA{' '}
                    <span class="mono">{shortSha(chStatus()?.sha256)}</span>
                  </span>
                </div>

                <div style={sx({ display: 'flex', gap: 8 })}>
                  <Btn variant="secondary" icon="info" onClick={() => setConfirm('preview')}>升级预览</Btn>
                  <Btn
                    variant="primary"
                    icon="update"
                    onClick={() => setConfirm('apply')}
                    disabled={!chStatus()?.canApply || busy()}
                    style={sx({ flex: 1 })}
                  >
                    一键升级到 {targetVersion() ?? s().currentVersion}
                  </Btn>
                </div>

                {/* 升级进度（apply 进行中或终态保留） */}
                <Show when={progress()}>
                  {(p) => (
                    <div>
                      <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 12, marginBottom: 6 })}>
                        <span style={sx({ color: p().failed ? 'var(--error)' : 'var(--text-2)' })}>
                          {p().phase}{p().failed ? '（升级未完成）' : ''}
                        </span>
                        <span class="mono muted">{p().percent}%</span>
                      </div>
                      <div class="bar" style={sx({ height: 7 })}>
                        <i style={sx({ width: `${p().percent}%`, background: p().failed ? 'var(--error)' : 'var(--grad-brand)' })} />
                      </div>
                    </div>
                  )}
                </Show>

                <div style={sx({ padding: '10px 12px', borderRadius: 10, background: 'var(--warning-soft)', color: 'var(--warning)', fontSize: 11.5 })}>
                  ⚠ 升级前自动创建数据库备份；升级异步执行，期间服务短暂不可用。
                </div>

                {/* 维护模式开关 */}
                <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '12px 14px', borderRadius: 12, background: settings()?.maintenanceMode ? 'var(--warning-soft)' : 'var(--surface-sunken)' })}>
                  <div>
                    <div style={sx({ fontWeight: 600, fontSize: 13 })}>维护模式</div>
                    <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 2 })}>
                      {settings()?.maintenanceMode ? '已开启 · /api/* 返回 503' : '关闭 · 服务正常，建议升级前开启'}
                    </div>
                  </div>
                  <Switch checked={!!settings()?.maintenanceMode} onChange={toggleMaintenance} disabled={maintBusy()} />
                </div>
              </div>
            )}
          </Show>
        </Panel>

        <Panel
          title="CHANGELOG"
          sub={changelog()?.available ? `${changelog()!.base ?? ''} → ${changelog()!.head ?? targetVersion() ?? ''}` : ''}
        >
          <Show when={changelog.state !== 'pending'} fallback={<Loading />}>
            <Show
              when={changelogGroups().length > 0}
              fallback={
                <Show
                  when={chStatus()?.releaseNotes}
                  fallback={<p class="muted" style={sx({ fontSize: 12.5 })}>{changelog()?.reason ?? '暂无可比对的 CHANGELOG。'}</p>}
                >
                  <Show
                    when={releaseNotesSections().sections.length > 0}
                    fallback={
                      <pre class="muted" style={sx({ fontSize: 12, whiteSpace: 'pre-wrap', margin: 0, lineHeight: 1.6 })}>
                        {chStatus()!.releaseNotes}
                      </pre>
                    }
                  >
                    <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
                      <Show when={releaseNotesSections().intro}>
                        <p class="muted" style={sx({ fontSize: 12, lineHeight: 1.6, margin: 0 })}>
                          {releaseNotesSections().intro}
                        </p>
                      </Show>
                      <For each={releaseNotesSections().sections}>
                        {(g) => (
                          <div>
                            <div style={sx({ fontSize: 12, fontWeight: 700, color: g.color, marginBottom: 6 })}>{g.label}</div>
                            <ul style={sx({ margin: 0, paddingLeft: 18 })}>
                              <For each={g.items}>
                                {(it) => (
                                  <li style={sx({ fontSize: 12.5, marginBottom: 4, color: 'var(--text-2)' })}>{it}</li>
                                )}
                              </For>
                            </ul>
                          </div>
                        )}
                      </For>
                    </div>
                  </Show>
                </Show>
              }
            >
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
                <Show when={changelog()?.totalCommits != null}>
                  <div class="muted-3 mono" style={sx({ fontSize: 11 })}>
                    {changelog()!.totalCommits} commits · {changelog()!.contributors ?? 0} contributors
                  </div>
                </Show>
                <For each={changelogGroups()}>
                  {(g) => (
                    <div>
                      <div style={sx({ fontSize: 12, fontWeight: 700, color: g.color, marginBottom: 6 })}>
                        {g.label}（{g.items.length}）
                      </div>
                      <ul style={sx({ margin: 0, paddingLeft: 18 })}>
                        <For each={g.items}>
                          {(c) => (
                            <li style={sx({ fontSize: 12.5, marginBottom: 4, color: 'var(--text-2)' })}>
                              <Show when={c.scope}>
                                <span class="mono" style={sx({ color: 'var(--text-3)', marginRight: 4 })}>{c.scope}:</span>
                              </Show>
                              {c.subject} <code class="mono" style={sx({ fontSize: 10.5, color: 'var(--text-3)' })}>{c.sha.slice(0, 7)}</code>
                            </li>
                          )}
                        </For>
                      </ul>
                    </div>
                  )}
                </For>
                <Show when={changelog()?.compareUrl}>
                  <a class="mono" style={sx({ fontSize: 11.5, color: 'var(--accent)' })} href={changelog()!.compareUrl} target="_blank" rel="noopener">
                    GitHub compare →
                  </a>
                </Show>
              </div>
            </Show>
          </Show>
        </Panel>
      </div>

      {/* 版本历史 + 数据库备份 */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.2fr) minmax(0,1fr)', gap: 16 })}
      >
        <Panel title="版本历史" sub="升级 / 回滚审计">
          <Show when={history.state !== 'pending'} fallback={<Loading />}>
            <Show
              when={history()?.entries?.length}
              fallback={<p class="muted" style={sx({ fontSize: 12.5 })}>暂无升级记录。</p>}
            >
              <div style={sx({ overflowX: 'auto' })}>
                <table class="tbl">
                  <thead>
                    <tr><th>动作</th><th>版本</th><th>结果</th><th>耗时</th><th>通道</th><th>时间</th><th style={sx({ textAlign: 'right' })}>操作</th></tr>
                  </thead>
                  <tbody>
                    <For each={history()!.entries}>
                      {(h) => {
                        const isRollback = h.action === 'rollback' || h.outcome === 'rolled_back';
                        const dur = auditDurSecs(h);
                        const canRollback = !!h.fromVersion && h.fromVersion !== status()?.currentVersion;
                        return (
                          <tr>
                            <td><Badge variant={isRollback ? 'warning' : 'accent'}>{isRollback ? '回滚' : '升级'}</Badge></td>
                            <td class="mono" style={sx({ fontSize: 11.5 })}>{h.fromVersion}→{h.toVersion}</td>
                            <td>
                              <Badge
                                variant={h.outcome === 'success' ? 'success' : h.outcome === 'failed' ? 'error' : h.outcome === 'rolled_back' ? 'warning' : 'info'}
                                dot
                              >
                                {h.outcome === 'success' ? '成功' : h.outcome === 'failed' ? '失败' : h.outcome === 'rolled_back' ? '已回滚' : '进行中'}
                              </Badge>
                            </td>
                            <td class="mono">{dur != null ? `${dur}s` : '—'}</td>
                            <td class="mono muted" style={sx({ fontSize: 11.5 })}>{h.channel}</td>
                            <td class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtAgo(h.startedAt)}</td>
                            <td style={sx({ textAlign: 'right' })}>
                              <Show when={canRollback}>
                                <Btn
                                  size="xs"
                                  variant="outline"
                                  icon="rotate"
                                  onClick={() => { setRollbackTarget(h.fromVersion); setConfirm('rollback'); }}
                                >
                                  回滚
                                </Btn>
                              </Show>
                            </td>
                          </tr>
                        );
                      }}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>
          </Show>
        </Panel>

        <Panel
          title="数据库备份"
          sub="升级 / 每日 / 手动"
          right={<Btn size="sm" variant="primary" icon="plus" onClick={createBackup} disabled={busy()}>立即备份</Btn>}
        >
          <Show when={backups.state !== 'pending'} fallback={<Loading />}>
            <Show
              when={backups()?.backups?.length}
              fallback={<p class="muted" style={sx({ fontSize: 12.5 })}>暂无备份。</p>}
            >
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 8, maxHeight: 320, overflowY: 'auto' })}>
                <For each={backups()!.backups}>
                  {(b) => (
                    <div style={sx({ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 12px', borderRadius: 10, background: 'var(--surface-sunken)' })}>
                      <Icon name="db" size={16} style={sx({ color: 'var(--accent)', flex: 'none' })} />
                      <div style={sx({ flex: 1, minWidth: 0 })}>
                        <div class="mono" style={sx({ fontSize: 11.5, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{b.name}</div>
                        <div class="muted-3" style={sx({ fontSize: 10.5 })}>
                          {fmtBytes(b.sizeBytes)} · {fmtAgo(b.createdAt)}{b.version ? ` · ${b.version}` : ''}
                        </div>
                      </div>
                      <Badge variant="default">{BACKUP_KIND_LABEL[b.kind] ?? b.kind}</Badge>
                      <IconBtn name="download" title="下载" size={15} onClick={() => downloadBackup(b)} />
                      <Btn size="xs" variant="outline" onClick={() => setConfirm({ kind: 'restore', backup: b })}>恢复</Btn>
                    </div>
                  )}
                </For>
              </div>
              <div class="muted-3" style={sx({ fontSize: 11, marginTop: 10 })}>
                总占用 <span class="mono">{fmtBytes(backups()!.totalBytes)}</span> · 阈值{' '}
                <span class="mono">{fmtBytes(backups()!.thresholdBytes)}</span> · 共 {fmtNum(backups()!.backups.length)} 个
              </div>
              {/* 备份大小分布（按时间倒序最近若干个） */}
              <Show when={backups()!.backups.length > 1}>
                <div style={sx({ marginTop: 12 })}>
                  <div class="eyebrow" style={sx({ marginBottom: 8 })}>备份大小（最近）</div>
                  <BarChart
                    horizontal
                    fmtV={(v: number) => fmtBytes(v)}
                    data={backups()!.backups.slice(0, 6).map((b) => ({
                      label: BACKUP_KIND_LABEL[b.kind] ?? b.kind,
                      value: b.sizeBytes,
                    }))}
                  />
                </div>
              </Show>
            </Show>
          </Show>
        </Panel>
      </div>

      {/* 确认一键更新 */}
      <Confirm
        open={confirm() === 'apply'}
        onClose={() => setConfirm(null)}
        onConfirm={doApply}
        loading={busy()}
        title="确认一键更新"
        confirmText="开始升级"
        body={`将通过 ${channel()} 通道升级到 ${targetVersion() ?? '—'}。升级前自动备份，过程异步且服务短暂不可用，期间请勿刷新页面。`}
      />

      {/* 升级预览（dry-run） */}
      <Confirm
        open={confirm() === 'preview'}
        onClose={() => setConfirm(null)}
        onConfirm={() => setConfirm(null)}
        title="升级预览（dry-run）"
        confirmText="知道了"
        body={
          <div>
            <p style={sx({ marginTop: 0 })}>目标 {targetVersion() ?? '—'}（{channel()} 通道）将执行：</p>
            <ul style={sx({ paddingLeft: 18, fontSize: 13, lineHeight: 1.8, margin: 0 })}>
              <li>VACUUM INTO 备份当前 SQLite</li>
              <li>下载 tarball（{chStatus() ? fmtBytes(chStatus()!.tarballSize) : '—'}）并校验 sha256</li>
              <li>原子替换二进制与 static 目录</li>
              <li>fork-exec 自重启 + 30s 健康监测</li>
            </ul>
            <Show when={changelog()?.available && changelog()?.totalCommits != null}>
              <p style={sx({ fontSize: 12.5, marginBottom: 0 })} class="muted">
                本次包含 {changelog()!.totalCommits} commits。
              </p>
            </Show>
          </div>
        }
      />

      {/* 回滚二次确认 */}
      <Confirm
        open={confirm() === 'rollback'}
        onClose={() => setConfirm(null)}
        onConfirm={doRollback}
        loading={busy()}
        danger
        title="确认回滚版本"
        confirmText="尝试回滚"
        body={
          <div>
            <p style={sx({ marginTop: 0 })}>
              将从 <span class="mono">{status()?.currentVersion ?? '—'}</span> 切换到{' '}
              <span class="mono">{rollbackTarget() ?? previousEntry()?.fromVersion ?? '—'}</span>。
            </p>
            <p style={sx({ fontSize: 12, color: 'var(--warning)' })}>
              ⚠ 走 /rollback 端点，跳过 semver 上升校验，从 GitHub 拉 target tag 的 release metadata 再走标准 apply 流程（sha256 校验 / DB 备份 / fork-exec 自重启）。如目标 tag 在 GitHub 已删除会失败。
            </p>
          </div>
        }
      />

      {/* 备份恢复二次确认 */}
      <Confirm
        open={!!confirm() && typeof confirm() === 'object' && (confirm() as { kind: string }).kind === 'restore'}
        onClose={() => setConfirm(null)}
        onConfirm={() => {
          const c = confirm();
          if (typeof c === 'object' && c?.kind === 'restore') void restoreBackup(c.backup);
        }}
        loading={busy()}
        danger
        title="从备份恢复"
        confirmText="确认恢复"
        body={
          <div>
            <p style={sx({ marginTop: 0 })}>
              将用 <span class="mono">{typeof confirm() === 'object' ? (confirm() as { backup: BackupEntry }).backup.name : ''}</span> 覆盖当前数据库（会先创建 pre-restore 兜底备份）。
            </p>
            <p style={sx({ fontSize: 12, color: 'var(--warning)' })}>
              恢复为在线页拷贝，完成后建议重启后端进程使连接池重读数据。此操作不可中途取消。
            </p>
          </div>
        }
      />
    </div>
  );
}
