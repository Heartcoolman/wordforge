import { createSignal, createMemo, Show, For, onMount } from 'solid-js';
import { Portal } from 'solid-js/web';
import { uiStore } from '@/stores/ui';
import {
  packsApi,
  type AdminPackEntry,
  type PackChannel,
  type PackSummary,
  type PackStatsEntry,
  type ResourcePackVersion,
} from '@/api/packs';
import { ApiError } from '@/api/http';
import './resourcePacks.css';

/**
 * 资源包管理页（对齐设计稿 resource-packs.html）。
 * 多包多版本 · 三通道（stable / beta / internal）· Ed25519 + SHA-256 · 激活 SSE 广播。
 *
 * 真实架构裁剪：安装 outcome 仅 installed / verify_failed / rollback 三态。
 * 全部功能、API、状态、handler 与旧版一致，仅换皮对齐设计结构。
 * 对接 /api/admin/resource-packs（详见 api/packs.ts）。
 */
type ChannelFilter = PackChannel | 'all';

const CH_META: Record<PackChannel, { label: string; cls: string }> = {
  stable: { label: 'STABLE', cls: 'is-stable' },
  beta: { label: 'BETA', cls: 'is-beta' },
  internal: { label: 'INTERNAL', cls: 'is-internal' },
};

export default function ResourcePacksPage() {
  const [loading, setLoading] = createSignal(true);
  const [entries, setEntries] = createSignal<AdminPackEntry[]>([]);
  const [summary, setSummary] = createSignal<PackSummary | null>(null);
  const [filter, setFilter] = createSignal<ChannelFilter>('all');
  const [openPack, setOpenPack] = createSignal<string | null>(null);

  // 上传抽屉
  const [drawerOpen, setDrawerOpen] = createSignal(false);
  const [upPackId, setUpPackId] = createSignal('');
  const [upVersion, setUpVersion] = createSignal('');
  const [upChannel, setUpChannel] = createSignal<PackChannel>('beta');
  const [upMinApp, setUpMinApp] = createSignal('');
  const [upDesc, setUpDesc] = createSignal('');
  const [upFile, setUpFile] = createSignal<File | null>(null);
  const [upSha, setUpSha] = createSignal<string | null>(null);
  const [dragOver, setDragOver] = createSignal(false);
  const [uploading, setUploading] = createSignal(false);
  const [progress, setProgress] = createSignal(0);

  // 激活确认 / 软删除 / 统计弹窗
  const [activate, setActivate] = createSignal<{ pack: AdminPackEntry; v: ResourcePackVersion } | null>(null);
  const [del, setDel] = createSignal<{ pack: AdminPackEntry; v: ResourcePackVersion } | null>(null);
  const [statsPack, setStatsPack] = createSignal<AdminPackEntry | null>(null);
  const [statsRows, setStatsRows] = createSignal<PackStatsEntry[] | null>(null);

  let fileInput: HTMLInputElement | undefined;

  async function refresh() {
    setLoading(true);
    try {
      const [list, sum] = await Promise.all([
        packsApi.list(),
        packsApi.summary().catch(() => null),
      ]);
      setEntries(list);
      setSummary(sum);
      if (list.length > 0 && !openPack()) setOpenPack(list[0].packId);
    } catch (err) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  onMount(refresh);

  // —— 派生：过滤后的 pack 列表（按通道有版本筛选）——
  const visiblePacks = createMemo(() => {
    const f = filter();
    if (f === 'all') return entries();
    return entries().filter((e) => e.versions.some((v) => v.channel === f && !v.deactivatedAt));
  });

  const counts = createMemo(() => {
    const all = entries();
    const byCh = (ch: PackChannel) =>
      all.filter((e) => e.versions.some((v) => v.channel === ch && !v.deactivatedAt)).length;
    return { all: all.length, stable: byCh('stable'), beta: byCh('beta'), internal: byCh('internal') };
  });

  // 某 pack 某通道的「当前激活版本」（active 表为准）
  const channelActive = (pack: AdminPackEntry, ch: PackChannel) => {
    const ver = pack.active[ch];
    if (!ver) return null;
    return pack.versions.find((v) => v.version === ver && v.channel === ch) ?? null;
  };

  const sortedVersions = (pack: AdminPackEntry) =>
    [...pack.versions].sort((a, b) => (b.publishedAt > a.publishedAt ? 1 : -1));

  const isActive = (pack: AdminPackEntry, v: ResourcePackVersion) =>
    pack.active[v.channel] === v.version && !v.deactivatedAt;

  // —— 上传 —— XHR 真实进度
  async function pickFile(f: File | null) {
    setUpFile(f);
    setUpSha(null);
    if (!f) return;
    if (f.size > 4 * 1024 * 1024) {
      uiStore.toast.warning('文件超过 4 MiB 上限');
      setUpFile(null);
      return;
    }
    // 真实 SHA-256（WebCrypto），仅展示用；最终签名/落盘由服务端完成
    try {
      const buf = await f.arrayBuffer();
      const hash = await crypto.subtle.digest('SHA-256', buf);
      const hex = Array.from(new Uint8Array(hash)).map((b) => b.toString(16).padStart(2, '0')).join('');
      setUpSha(hex);
    } catch {
      setUpSha(null);
    }
  }

  async function doUpload() {
    if (!upPackId().trim() || !upVersion().trim() || !upFile()) {
      uiStore.toast.warning('请填写 pack_id、版本号并选择 payload 文件');
      return;
    }
    setUploading(true);
    setProgress(0);
    try {
      await packsApi.uploadVersion(
        upPackId().trim(),
        {
          version: upVersion().trim(),
          channel: upChannel(),
          minAppVersion: upMinApp().trim() || undefined,
          description: upDesc().trim() || undefined,
        },
        upFile()!,
        (frac) => setProgress(Math.round(frac * 100)),
      );
      uiStore.toast.success('上传成功', '已自动 SHA-256 + Ed25519 签名落盘');
      closeDrawer();
      await refresh();
    } catch (err) {
      if (err instanceof ApiError && err.code === 'PACK_VERSION_EXISTS') {
        uiStore.toast.error('该版本号已存在', '请改用新版本号；同版本号不可覆盖，避免线上验签失败');
      } else {
        uiStore.toast.error('上传失败', err instanceof Error ? err.message : '');
      }
    } finally {
      setUploading(false);
    }
  }

  function openDrawer(packId?: string) {
    setUpPackId(packId ?? '');
    setUpVersion('');
    setUpChannel('beta');
    setUpMinApp('');
    setUpDesc('');
    setUpFile(null);
    setUpSha(null);
    setProgress(0);
    setDrawerOpen(true);
  }
  function closeDrawer() {
    if (uploading()) return;
    setDrawerOpen(false);
  }

  // —— 激活 —— 展示 audienceClients 在线 SSE 客户端数
  async function doActivate() {
    const t = activate();
    if (!t) return;
    setActivate(null);
    try {
      const res = await packsApi.setActive(t.pack.packId, t.v.channel, t.v.version);
      uiStore.toast.success(
        `已激活 ${t.pack.packId} ${t.v.version}（${t.v.channel}）`,
        `已通过 SSE 广播到 ${res.audienceClients} 个在线客户端`,
      );
      await refresh();
    } catch (err) {
      uiStore.toast.error('激活失败', err instanceof Error ? err.message : '');
    }
  }

  async function doDeactivate() {
    const t = del();
    if (!t) return;
    setDel(null);
    try {
      await packsApi.deactivateVersion(t.pack.packId, t.v.version);
      uiStore.toast.success('已停用版本');
      await refresh();
    } catch (err) {
      uiStore.toast.error('停用失败', err instanceof Error ? err.message : '');
    }
  }

  // —— 统计弹窗 —— install_log 聚合（version × outcome）
  async function openStats(pack: AdminPackEntry) {
    setStatsPack(pack);
    setStatsRows(null);
    try {
      const res = await packsApi.stats(pack.packId);
      setStatsRows(res.stats);
    } catch (err) {
      uiStore.toast.error('加载统计失败', err instanceof Error ? err.message : '');
      setStatsRows([]);
    }
  }

  // —— 格式化 ——
  const fmtSize = (b: number) => {
    if (b < 1024) return `${b} B`;
    if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)} KiB`;
    return `${(b / 1024 / 1024).toFixed(2)} MiB`;
  };
  const fmtNum = (n: number) => n.toLocaleString('en-US');
  const fmtAge = (iso: string) => {
    const diff = Date.now() - new Date(iso).getTime();
    const h = Math.floor(diff / 3_600_000);
    if (h < 1) return '刚刚';
    if (h < 24) return `${h} 小时前`;
    return `${Math.floor(h / 24)} 天前`;
  };
  const fmtPub = (iso: string) => {
    const d = new Date(iso);
    const p = (n: number) => String(n).padStart(2, '0');
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  };
  const shaShort = (h: string) => `${h.slice(0, 8)} ··· ${h.slice(-4)}`;
  function copySha(h: string) {
    navigator.clipboard?.writeText(h).then(
      () => uiStore.toast.success('已复制 SHA-256'),
      () => {},
    );
  }

  // 激活弹窗：当前激活版本 & 在线受众
  const activeCurrent = createMemo(() => {
    const t = activate();
    if (!t) return null;
    return channelActive(t.pack, t.v.channel);
  });

  const arrow = (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6" /></svg>
  );

  return (
    <div class="rp">
      {/* —— 页头 —— */}
      <div class="page-header">
        <div class="row spread">
          <div>
            <h1 class="page-title">资源包</h1>
            <p class="page-desc">
              多包多版本 · Stable / Beta / Internal 三通道 · Ed25519 签名 + SHA-256 校验 ·
              切激活通过 SSE 实时通告所有在线客户端（5 分钟去重）。单包硬上限 4 MiB。
            </p>
          </div>
          <div class="row gap-2">
            <button class="btn btn-secondary" onClick={refresh} disabled={loading()}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" /></svg>
              刷新
            </button>
            <button class="btn btn-primary" onClick={() => openDrawer()}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" /></svg>
              上传新版
            </button>
          </div>
        </div>
      </div>

      {/* —— KPI stats —— */}
      <div class="rp-stats">
        <div class="rp-stat-card">
          <div class="l">资源包总数</div>
          <div class="v">{summary()?.totalPacks ?? entries().length}</div>
          <div class="meta">本月新增 {summary()?.newPacksThisMonth ?? 0}</div>
        </div>
        <div class="rp-stat-card is-info">
          <div class="l">版本总数</div>
          <div class="v">{summary()?.totalVersions ?? entries().reduce((a, e) => a + e.versions.length, 0)}</div>
          <Show when={summary()} fallback={<div class="meta">—</div>}>
            {(s) => <div class="meta">stable {s().versionsByChannel.stable} · beta {s().versionsByChannel.beta} · internal {s().versionsByChannel.internal}</div>}
          </Show>
        </div>
        <div class="rp-stat-card is-ok">
          <div class="l">今日下载</div>
          <div class="v">{fmtNum(summary()?.installsToday ?? 0)}</div>
          <Show when={summary()} fallback={<div class="meta">—</div>}>
            {(s) => (
              <div class="meta">
                成功 {fmtNum(s().installsTodaySuccess)}
                {s().installsToday > 0 && ` (${((s().installsTodaySuccess / s().installsToday) * 100).toFixed(1)}%)`}
              </div>
            )}
          </Show>
        </div>
        <div class="rp-stat-card is-warn">
          <div class="l">失败率 (7d)</div>
          <div class="v">
            {((summary()?.failureRate7d ?? 0) * 100).toFixed(1)}
            <span style={{ 'font-size': '14px', color: 'var(--content-tertiary)', 'font-weight': '500' }}>%</span>
          </div>
          <Show when={summary()} fallback={<div class="meta">—</div>}>
            {(s) => <div class="meta">verify_failed {s().failures7dByOutcome.verify_failed} · rollback {s().failures7dByOutcome.rollback}</div>}
          </Show>
        </div>
      </div>

      {/* —— 通道筛选 —— */}
      <div class="rp-tabs" role="tablist">
        <button role="tab" classList={{ 'is-active': filter() === 'all' }} aria-selected={filter() === 'all'} onClick={() => setFilter('all')}>
          全部 <span class="count">{counts().all}</span>
        </button>
        <button role="tab" data-ch="stable" classList={{ 'is-active': filter() === 'stable' }} aria-selected={filter() === 'stable'} onClick={() => setFilter('stable')}>
          <span class="dot" /> Stable <span class="count">{counts().stable}</span>
        </button>
        <button role="tab" data-ch="beta" classList={{ 'is-active': filter() === 'beta' }} aria-selected={filter() === 'beta'} onClick={() => setFilter('beta')}>
          <span class="dot" /> Beta <span class="count">{counts().beta}</span>
        </button>
        <button role="tab" data-ch="internal" classList={{ 'is-active': filter() === 'internal' }} aria-selected={filter() === 'internal'} onClick={() => setFilter('internal')}>
          <span class="dot" /> Internal <span class="count">{counts().internal}</span>
        </button>
      </div>

      {/* —— pack 列表 —— */}
      <Show
        when={!loading()}
        fallback={<div class="rp-list"><div class="rp-skel" /><div class="rp-skel" /><div class="rp-skel" /></div>}
      >
        <Show
          when={visiblePacks().length > 0}
          fallback={<div class="rp-empty"><div class="t">暂无资源包</div><div class="d">点击右上角「上传新版」创建第一个 pack 版本</div></div>}
        >
          <div class="rp-list">
            <For each={visiblePacks()}>{(pack) => {
              const open = () => openPack() === pack.packId;
              return (
                <article class="rp-pack" classList={{ 'is-open': open() }}>
                  <div class="rp-pack-head">
                    <button
                      type="button"
                      class="rp-pack-id"
                      onClick={() => setOpenPack(open() ? null : pack.packId)}
                    >
                      <div class="pic">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" /><polyline points="3.27 6.96 12 12.01 20.73 6.96" /><line x1="12" y1="22.08" x2="12" y2="12" /></svg>
                      </div>
                      <div class="nm">
                        {pack.description || pack.packId}
                        <span class="id">{pack.packId}</span>
                      </div>
                      <div class="arr">{arrow}</div>
                    </button>

                    <For each={['stable', 'beta', 'internal'] as PackChannel[]}>{(ch) => {
                      const av = channelActive(pack, ch);
                      return (
                        <div class={`rp-ch-slot ${CH_META[ch].cls}`} classList={{ 'is-empty': !av }}>
                          <div class="l">{CH_META[ch].label}</div>
                          <Show when={av} fallback={<><div class="vv">—</div><div class="size">未发布</div></>}>
                            {(v) => (<>
                              <div class="vv">v{v().version} <span class="age">{fmtAge(v().publishedAt)}</span></div>
                              <div class="size">{fmtSize(v().sizeBytes)}</div>
                            </>)}
                          </Show>
                        </div>
                      );
                    }}</For>

                    <div class="rp-pack-totals">
                      <div class="v">{fmtNum(pack.totalInstalls)}</div>
                      <div class="l">总下载</div>
                    </div>
                  </div>

                  {/* 展开：版本表 */}
                  <div class="rp-versions">
                    <div class="rp-vrow vrow-head">
                      <div>版本</div><div>通道</div><div>大小</div><div>SHA-256</div>
                      <div data-col="sig">签名</div><div data-col="min">min app</div><div>发布时间</div><div>状态</div><div />
                    </div>
                    <For each={sortedVersions(pack)}>{(v) => {
                      const active = isActive(pack, v);
                      const replaced = !v.deactivatedAt && !active;
                      return (
                        <div class="rp-vrow">
                          <div class="ver">{v.version}</div>
                          <div class={`ch ch-${v.channel}`}>{CH_META[v.channel].label}</div>
                          <div class="sz">{fmtSize(v.sizeBytes)}</div>
                          <div class="sha">
                            <span class="h">{shaShort(v.sha256)}</span>
                            <button class="copy" title="复制 SHA-256" onClick={() => copySha(v.sha256)}>
                              <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>
                            </button>
                          </div>
                          <div data-col="sig">
                            <span class="sig" classList={{ 'is-none': !v.signature }}>
                              <span class="dot" /> {v.signature ? v.signatureAlg : '未签名'}
                            </span>
                          </div>
                          <div class="min" data-col="min">{v.minAppVersion ? `≥ ${v.minAppVersion}` : '—'}</div>
                          <div class="pub">{fmtPub(v.publishedAt)} <span>{fmtAge(v.publishedAt)}</span></div>
                          <div>
                            <span
                              class="st"
                              classList={{ 'st-active': active, 'st-deactivated': !!v.deactivatedAt, 'st-inactive': replaced }}
                            >
                              {active ? '激活中' : v.deactivatedAt ? '已停用' : '已被替换'}
                            </span>
                          </div>
                          <div class="ops">
                            <button onClick={() => openStats(pack)}>统计</button>
                            <Show when={replaced}>
                              <button class="primary" onClick={() => setActivate({ pack, v })}>激活</button>
                            </Show>
                            <Show when={!v.deactivatedAt}>
                              <button class="danger" onClick={() => setDel({ pack, v })}>删除</button>
                            </Show>
                          </div>
                        </div>
                      );
                    }}</For>
                  </div>

                  {/* 展开：近 7 天安装结果（真实三态） */}
                  <div class="rp-pack-stats">
                    <h5>近 7 天安装结果 · install_log 聚合</h5>
                    {(() => {
                      const o = pack.outcomes7d;
                      const total = o.installed + o.verify_failed + o.rollback;
                      const pct = (n: number) => (total > 0 ? Math.max(2, Math.round((n / total) * 100)) : 0);
                      return (
                        <div class="rp-outcome-bars">
                          <div class="rp-outcome is-success"><div class="l">installed</div><div class="v">{fmtNum(o.installed)}</div><div class="bar"><span style={{ width: `${pct(o.installed)}%` }} /></div></div>
                          <div class="rp-outcome is-verify-failed"><div class="l">verify_failed</div><div class="v">{fmtNum(o.verify_failed)}</div><div class="bar"><span style={{ width: `${pct(o.verify_failed)}%` }} /></div></div>
                          <div class="rp-outcome is-rollback"><div class="l">rollback</div><div class="v">{fmtNum(o.rollback)}</div><div class="bar"><span style={{ width: `${pct(o.rollback)}%` }} /></div></div>
                        </div>
                      );
                    })()}
                  </div>
                </article>
              );
            }}</For>
          </div>
        </Show>
      </Show>

      {/* ===== 上传抽屉 ===== */}
      <Show when={drawerOpen()}>
        <Portal>
          <div class="rp-drawer-backdrop" onClick={closeDrawer} />
          <aside class="rp-drawer is-open" role="dialog" aria-modal="true" aria-label="上传新版资源包">
            <div class="rp-drawer-head">
              <div class="ic">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="17 8 12 3 7 8" /><line x1="12" y1="3" x2="12" y2="15" /></svg>
              </div>
              <h3>上传新版</h3>
              <span class="sub">POST /api/admin/resource-packs/:pack_id/versions</span>
              <button class="x" aria-label="关闭" onClick={closeDrawer}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></svg>
              </button>
            </div>

            <div class="rp-drawer-body">
              <div class="rp-field">
                <div class="rp-field-label"><label for="rp-pack">资源包 ID (pack_id) *</label><span class="hint">snake_case · 不可改</span></div>
                <input
                  id="rp-pack" class="rp-input is-mono" list="rp-pack-options"
                  placeholder="例 wordbook-en-gre / amas-presets-default"
                  value={upPackId()} disabled={uploading()}
                  onInput={(e) => setUpPackId(e.currentTarget.value)}
                />
                <datalist id="rp-pack-options">
                  <For each={entries()}>{(e) => <option value={e.packId} />}</For>
                </datalist>
              </div>

              <div class="rp-field">
                <div class="rp-field-label"><label for="rp-ver">版本号 (semver) *</label><span class="hint">例 3.3.0-beta.2 / 1.0.0</span></div>
                <input id="rp-ver" class="rp-input is-mono" placeholder="major.minor.patch[-pre]" value={upVersion()} disabled={uploading()} onInput={(e) => setUpVersion(e.currentTarget.value)} />
              </div>

              <div class="rp-field">
                <div class="rp-field-label"><label>通道 *</label><span class="hint">stable / beta / internal</span></div>
                <div class="rp-channel-pick">
                  <For each={['stable', 'beta', 'internal'] as PackChannel[]}>{(ch) => (
                    <label data-ch={ch} classList={{ 'is-selected': upChannel() === ch }} onClick={() => !uploading() && setUpChannel(ch)}>
                      <input type="radio" name="rp-ch" value={ch} checked={upChannel() === ch} />
                      <span class="nm">{ch === 'stable' ? 'Stable' : ch === 'beta' ? 'Beta' : 'Internal'}</span>
                      <span class="d">{ch === 'stable' ? '生产用户' : ch === 'beta' ? '早期访问' : '仅内部'}</span>
                    </label>
                  )}</For>
                </div>
              </div>

              <div class="rp-field">
                <div class="rp-field-label"><label for="rp-min">最低 app 版本 (minAppVersion)</label><span class="hint">可选</span></div>
                <input id="rp-min" class="rp-input is-mono" placeholder="例 0.7.0" value={upMinApp()} disabled={uploading()} onInput={(e) => setUpMinApp(e.currentTarget.value)} />
              </div>

              <div class="rp-field">
                <div class="rp-field-label"><label for="rp-desc">说明 (description)</label><span class="hint">可选 · 仅给 admin 看</span></div>
                <textarea id="rp-desc" class="rp-textarea" placeholder="例：补全 GRE 高频词 v2 列表 1,028 条 + 修正 17 个释义" value={upDesc()} disabled={uploading()} onInput={(e) => setUpDesc(e.currentTarget.value)} />
              </div>

              <div
                class="rp-drop"
                classList={{ 'is-over': dragOver(), 'has-file': !!upFile() }}
                onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
                onDragLeave={() => setDragOver(false)}
                onDrop={(e) => { e.preventDefault(); setDragOver(false); if (e.dataTransfer?.files.length) pickFile(e.dataTransfer.files[0]); }}
              >
                <div class="ic"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="17 8 12 3 7 8" /><line x1="12" y1="3" x2="12" y2="15" /></svg></div>
                <div class="nm">{upFile() ? upFile()!.name : '拖入 payload.json 或点击选择'}</div>
                <div class="d">{upFile() ? `${fmtSize(upFile()!.size)} · ${upFile()!.type || 'application/json'}` : 'application/json · 单包硬上限 4 MiB · raw body 上传'}</div>
                <button class="btn-pick" disabled={uploading()} onClick={() => fileInput?.click()}>选择文件</button>
                <input ref={fileInput} type="file" accept="application/json,.json" hidden onChange={(e) => pickFile(e.currentTarget.files?.[0] ?? null)} />
              </div>

              <div class="rp-sig-row">
                <span class="k">SHA-256</span>
                <span class="v">{upSha() ? `${upSha()!.slice(0, 16)} ··· ${upSha()!.slice(-8)}` : '— 等待文件 —'}</span>
                <span class="k">Ed25519 签名</span>
                <span class="v is-ok"><span class="ic"><span class="dot" /> 上传时由服务端签</span></span>
                <span class="k">大小</span>
                <span class="v">{upFile() ? fmtSize(upFile()!.size) : '—'}</span>
              </div>

              <Show when={uploading()}>
                <div class="rp-upload-progress">
                  <div class="ph"><span>正在上传…</span><strong>{progress()}%</strong></div>
                  <div class="bar"><span style={{ width: `${progress()}%` }} /></div>
                  <div class="ph"><span>raw body → 服务端 SHA-256 + Ed25519 签名 + 落盘</span></div>
                </div>
              </Show>
            </div>

            <div class="rp-drawer-foot">
              <span class="meta">默认上传后<strong style={{ color: 'var(--content)', 'font-weight': '600' }}>不会自动激活</strong>，需手动切换</span>
              <span class="spacer" />
              <button class="btn btn-ghost" disabled={uploading()} onClick={closeDrawer}>取消</button>
              <button class="btn btn-primary" disabled={uploading() || !upPackId().trim() || !upVersion().trim() || !upFile()} onClick={doUpload}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12" /></svg>
                上传并签名
              </button>
            </div>
          </aside>
        </Portal>
      </Show>

      {/* ===== 激活确认弹窗 ===== */}
      <Show when={activate()}>
        {(t) => (
          <Portal>
            <div class="rp-activate-backdrop" onClick={() => setActivate(null)}>
              <div class="rp-activate-modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
                <h3>切换激活版本？</h3>
                <p>切换激活版本会通过 SSE 立即广播给所有在线客户端。客户端会在下次下载时拿到新版本，老版本仍保留在 <code>static/packs/</code> 中可回滚。</p>
                <div class="diff">
                  <div class="drow">
                    <span class="l">当前激活</span>
                    <span>
                      <Show when={activeCurrent()} fallback={<span class="l">无</span>}>
                        {(cur) => <><strong>v{cur().version}</strong> · {fmtAge(cur().publishedAt)}</>}
                      </Show>
                    </span>
                    <span class="arr" />
                  </div>
                  <div class="drow"><span class="l">即将激活</span><span><strong>v{t().v.version}</strong> · {fmtAge(t().v.publishedAt)}</span><span class="arr">→</span></div>
                  <div class="drow"><span class="l">通道</span><span><strong>{CH_META[t().v.channel].label}</strong></span><span class="arr" /></div>
                  <div class="drow">
                    <span class="l">受众</span>
                    <span>在线客户端 ~ <strong>{summary()?.onlineClients ?? '—'}</strong> 个</span>
                    <span class="arr" />
                  </div>
                </div>
                <div class="sse-note">
                  <strong>SSE 通告范围：</strong>所有订阅了 {CH_META[t().v.channel].label} 通道的在线客户端
                  <Show when={summary()}>{(s) => <> (~{s().onlineClients})</>}</Show>。
                  5 分钟内重复切换会被 <code>state.try_mark_pack_broadcast</code> 去重，不会重复推送。
                </div>
                <div class="foot">
                  <button class="btn btn-ghost" onClick={() => setActivate(null)}>取消</button>
                  <button class="btn btn-primary" onClick={doActivate}>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12" /></svg>
                    切换并广播
                  </button>
                </div>
              </div>
            </div>
          </Portal>
        )}
      </Show>

      {/* ===== 软删除确认弹窗 ===== */}
      <Show when={del()}>
        {(t) => (
          <Portal>
            <div class="rp-activate-backdrop" onClick={() => setDel(null)}>
              <div class="rp-activate-modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
                <h3>确认停用版本？</h3>
                <p>停用 <code>{t().v.version}</code> 后将从客户端 manifest 中摘除（已下载客户端不受影响，软删除，文件保留可恢复）。</p>
                <div class="foot">
                  <button class="btn btn-ghost" onClick={() => setDel(null)}>取消</button>
                  <button class="btn btn-primary" onClick={doDeactivate}>确认停用</button>
                </div>
              </div>
            </div>
          </Portal>
        )}
      </Show>

      {/* ===== 安装统计弹窗（version × outcome 聚合） ===== */}
      <Show when={statsPack()}>
        {(p) => (
          <Portal>
            <div class="rp-stats-backdrop" onClick={() => setStatsPack(null)}>
              <div class="rp-stats-modal" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
                <div class="rp-stats-modal-head">
                  <h3>安装统计</h3>
                  <span class="sub">{p().packId} · GET /:packId/stats</span>
                  <button class="x" aria-label="关闭" onClick={() => setStatsPack(null)}>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></svg>
                  </button>
                </div>
                <div class="rp-stats-modal-body">
                  <Show when={statsRows()} fallback={<div class="rp-stats-empty">加载中…</div>}>
                    {(rows) => {
                      // (version, outcome) → 按 version 聚合三态
                      type Agg = { installed: number; verify_failed: number; rollback: number };
                      const byVer = new Map<string, Agg>();
                      rows().forEach((r) => {
                        const a = byVer.get(r.version) ?? { installed: 0, verify_failed: 0, rollback: 0 };
                        if (r.outcome === 'installed') a.installed += r.count;
                        else if (r.outcome === 'verify_failed') a.verify_failed += r.count;
                        else if (r.outcome === 'rollback') a.rollback += r.count;
                        byVer.set(r.version, a);
                      });
                      const list = Array.from(byVer.entries()).sort((x, y) => (y[0] > x[0] ? 1 : -1));
                      return (
                        <Show when={list.length > 0} fallback={<div class="rp-stats-empty">该资源包暂无 install_log 记录</div>}>
                          <div class="rp-stats-table">
                            <div class="rp-strow rp-strow-head"><div>版本</div><div>installed</div><div>verify_failed</div><div>rollback</div><div>合计</div></div>
                            <For each={list}>{([ver, a]) => (
                              <div class="rp-strow">
                                <div class="ver">{ver}</div>
                                <div class="ok">{fmtNum(a.installed)}</div>
                                <div class="vf">{fmtNum(a.verify_failed)}</div>
                                <div class="rb">{fmtNum(a.rollback)}</div>
                                <div class="tt">{fmtNum(a.installed + a.verify_failed + a.rollback)}</div>
                              </div>
                            )}</For>
                          </div>
                        </Show>
                      );
                    }}
                  </Show>
                </div>
                <div class="rp-stats-modal-foot">
                  <button class="btn btn-ghost" onClick={() => setStatsPack(null)}>关闭</button>
                </div>
              </div>
            </div>
          </Portal>
        )}
      </Show>
    </div>
  );
}
