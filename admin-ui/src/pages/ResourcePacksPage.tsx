import { createResource, createSignal, createMemo, For, Show, type JSX } from 'solid-js';
import {
  Btn,
  IconBtn,
  Badge,
  Card,
  Field,
  Modal,
  Confirm,
  StatCard,
  Panel,
  Progress,
  Empty,
  Loading,
  Seg,
  Tabs,
  Icon,
  BarChart,
  PageHead,
  sx,
  toast,
  fmtNum,
  fmtBytes,
  fmtAgo,
} from '@/components/wf';
import {
  packsApi,
  type AdminPackEntry,
  type PackChannel,
  type PackSummary,
  type PackStatsEntry,
  type ResourcePackVersion,
} from '@/api/packs';

/**
 * 资源包管理页（对齐设计稿 resource-packs）。
 * TTS 语音、词书、AMAS 预设等资源包的多版本 / 三通道管理与安装统计。
 * 三通道 stable / beta / internal · Ed25519 + SHA-256 · 切激活经 SSE 实时广播。
 */

type ChannelFilter = PackChannel | 'all';

const CHANNELS: PackChannel[] = ['stable', 'beta', 'internal'];

const CH_META: Record<PackChannel, { label: string; badge: 'success' | 'warning' | 'info'; desc: string }> = {
  stable: { label: 'Stable', badge: 'success', desc: '生产用户' },
  beta: { label: 'Beta', badge: 'warning', desc: '早期访问' },
  internal: { label: 'Internal', badge: 'info', desc: '仅内部' },
};

const CH_COLOR: Record<PackChannel, string> = {
  stable: 'var(--success)',
  beta: 'var(--warning)',
  internal: 'var(--info)',
};

const shaShort = (h: string) => `${h.slice(0, 12)} … ${h.slice(-6)}`;

function copySha(h: string) {
  navigator.clipboard?.writeText(h).then(
    () => toast.success('已复制 SHA-256'),
    () => {},
  );
}

export default function ResourcePacksPage() {
  const [entries, { refetch: refetchEntries }] = createResource<AdminPackEntry[]>(() => packsApi.list());
  const [summary, { refetch: refetchSummary }] = createResource<PackSummary | null>(() =>
    packsApi.summary().catch(() => null),
  );

  const [filter, setFilter] = createSignal<ChannelFilter>('all');

  // 上传弹窗
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
  let fileInput: HTMLInputElement | undefined;

  // 激活 / 停用 / 统计
  const [activate, setActivate] = createSignal<{ pack: AdminPackEntry; v: ResourcePackVersion } | null>(null);
  const [del, setDel] = createSignal<{ pack: AdminPackEntry; v: ResourcePackVersion } | null>(null);
  const [acting, setActing] = createSignal(false);
  const [statsPack, setStatsPack] = createSignal<AdminPackEntry | null>(null);
  const [statsRows, setStatsRows] = createSignal<PackStatsEntry[] | null>(null);

  async function refresh() {
    await Promise.all([refetchEntries(), refetchSummary()]);
  }

  const allPacks = () => entries() ?? [];

  const counts = createMemo(() => {
    const all = allPacks();
    const byCh = (ch: PackChannel) =>
      all.filter((e) => e.versions.some((v) => v.channel === ch && !v.deactivatedAt)).length;
    return { all: all.length, stable: byCh('stable'), beta: byCh('beta'), internal: byCh('internal') };
  });

  const visiblePacks = createMemo(() => {
    const f = filter();
    const all = allPacks();
    if (f === 'all') return all;
    return all.filter((e) => e.versions.some((v) => v.channel === f && !v.deactivatedAt));
  });

  // 某 pack 某通道当前激活版本（active 表为准）
  const channelActive = (pack: AdminPackEntry, ch: PackChannel): ResourcePackVersion | null => {
    const ver = pack.active[ch];
    if (!ver) return null;
    return pack.versions.find((v) => v.version === ver && v.channel === ch) ?? null;
  };

  const sortedVersions = (pack: AdminPackEntry) =>
    [...pack.versions].sort((a, b) => (b.publishedAt > a.publishedAt ? 1 : -1));

  const isActive = (pack: AdminPackEntry, v: ResourcePackVersion) =>
    pack.active[v.channel] === v.version && !v.deactivatedAt;

  // —— 上传 —— WebCrypto 真实 SHA-256（展示用）
  async function pickFile(f: File | null) {
    setUpFile(f);
    setUpSha(null);
    if (!f) return;
    if (f.size > 4 * 1024 * 1024) {
      toast.warning('文件超过 4 MiB 上限');
      setUpFile(null);
      return;
    }
    try {
      const buf = await f.arrayBuffer();
      const hash = await crypto.subtle.digest('SHA-256', buf);
      const hex = Array.from(new Uint8Array(hash))
        .map((b) => b.toString(16).padStart(2, '0'))
        .join('');
      setUpSha(hex);
    } catch {
      setUpSha(null);
    }
  }

  function openUpload(packId?: string) {
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
  function closeUpload() {
    if (uploading()) return;
    setDrawerOpen(false);
  }

  async function doUpload() {
    if (!upPackId().trim() || !upVersion().trim() || !upFile()) {
      toast.warning('请填写 pack_id、版本号并选择 payload 文件');
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
      toast.success('上传成功', '已自动 SHA-256 + Ed25519 签名落盘');
      setDrawerOpen(false);
      await refresh();
    } catch (err) {
      const msg = err instanceof Error ? err.message : '';
      if (/EXIST/i.test(msg)) {
        toast.error('该版本号已存在', '请改用新版本号；同版本号不可覆盖');
      } else {
        toast.error('上传失败', msg);
      }
    } finally {
      setUploading(false);
    }
  }

  async function doActivate() {
    const t = activate();
    if (!t) return;
    setActing(true);
    try {
      const res = await packsApi.setActive(t.pack.packId, t.v.channel, t.v.version);
      toast.success(
        `已激活 ${t.pack.packId} ${t.v.version}（${t.v.channel}）`,
        `已通过 SSE 广播到 ${res.audienceClients} 个在线客户端`,
      );
      setActivate(null);
      await refresh();
    } catch (err) {
      toast.error('激活失败', err instanceof Error ? err.message : '');
    } finally {
      setActing(false);
    }
  }

  async function doDeactivate() {
    const t = del();
    if (!t) return;
    setActing(true);
    try {
      await packsApi.deactivateVersion(t.pack.packId, t.v.version);
      toast.success('已停用版本');
      setDel(null);
      await refresh();
    } catch (err) {
      toast.error('停用失败', err instanceof Error ? err.message : '');
    } finally {
      setActing(false);
    }
  }

  async function openStats(pack: AdminPackEntry) {
    setStatsPack(pack);
    setStatsRows(null);
    try {
      const res = await packsApi.stats(pack.packId);
      setStatsRows(res.stats);
    } catch (err) {
      toast.error('加载统计失败', err instanceof Error ? err.message : '');
      setStatsRows([]);
    }
  }

  // 激活弹窗：当前激活版本
  const activeCurrent = createMemo(() => {
    const t = activate();
    if (!t) return null;
    return channelActive(t.pack, t.v.channel);
  });

  const filterTabs = createMemo(() => {
    const c = counts();
    return [
      { value: 'all', label: '全部', count: c.all },
      { value: 'stable', label: 'Stable', count: c.stable },
      { value: 'beta', label: 'Beta', count: c.beta },
      { value: 'internal', label: 'Internal', count: c.internal },
    ];
  });

  return (
    <div>
      {/* —— 页头 —— */}
      <PageHead
        title="资源包"
        desc="TTS 语音、词书、AMAS 预设等资源包的多版本管理与安装统计。Stable / Beta / Internal 三通道 · Ed25519 签名 + SHA-256 校验 · 切激活经 SSE 实时广播所有在线客户端。单包硬上限 4 MiB。"
        right={
          <>
            <Btn variant="secondary" icon="refresh" onClick={refresh} disabled={entries.loading}>
              刷新
            </Btn>
            <Btn variant="primary" icon="upload" onClick={() => openUpload()}>
              上传新版
            </Btn>
          </>
        }
      />

      {/* —— KPI 卡 —— */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'repeat(4, minmax(0,1fr))', gap: 14, marginBottom: 18 })}
      >
        <StatCard
          tone="accent"
          icon="package"
          label="资源包总数"
          value={summary()?.totalPacks ?? allPacks().length}
          deltaLabel={`本月新增 ${summary()?.newPacksThisMonth ?? 0}`}
        />
        <StatCard
          tone="info"
          icon="layers"
          label="版本总数"
          value={summary()?.totalVersions ?? allPacks().reduce((a, e) => a + e.versions.length, 0)}
          deltaLabel={
            summary()
              ? `stable ${summary()!.versionsByChannel.stable} · beta ${summary()!.versionsByChannel.beta} · internal ${summary()!.versionsByChannel.internal}`
              : '—'
          }
        />
        <StatCard
          tone="success"
          icon="download"
          label="今日下载"
          value={fmtNum(summary()?.installsToday ?? 0)}
          deltaLabel={
            summary()
              ? `成功 ${fmtNum(summary()!.installsTodaySuccess)}${
                  summary()!.installsToday > 0
                    ? ` (${((summary()!.installsTodaySuccess / summary()!.installsToday) * 100).toFixed(1)}%)`
                    : ''
                }`
              : '—'
          }
        />
        <StatCard
          tone="warning"
          icon="shield"
          label="失败率 (7d)"
          value={((summary()?.failureRate7d ?? 0) * 100).toFixed(1)}
          unit="%"
          deltaLabel={
            summary()
              ? `verify_failed ${summary()!.failures7dByOutcome.verify_failed} · rollback ${summary()!.failures7dByOutcome.rollback}`
              : '—'
          }
        />
      </div>

      {/* —— 通道筛选 —— */}
      <div style={sx({ marginBottom: 16 })}>
        <Tabs tabs={filterTabs()} value={filter()} onChange={(v) => setFilter(v as ChannelFilter)} />
      </div>

      {/* —— pack 列表 —— */}
      <Show when={entries.state !== 'pending'} fallback={<Loading />}>
        <Show
          when={!entries.error}
          fallback={
            <Card>
              <Empty
                title="加载失败"
                desc={entries.error instanceof Error ? entries.error.message : '无法获取资源包列表'}
                icon="package"
                action={<Btn variant="secondary" icon="refresh" onClick={refresh}>重试</Btn>}
              />
            </Card>
          }
        >
          <Show
            when={visiblePacks().length > 0}
            fallback={
              <Card>
                <Empty
                  title="暂无资源包"
                  desc="点击右上角「上传新版」创建第一个 pack 版本"
                  icon="package"
                  action={<Btn variant="primary" icon="upload" onClick={() => openUpload()}>上传新版</Btn>}
                />
              </Card>
            }
          >
            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 16 })}>
              <For each={visiblePacks()}>
                {(pack) => (
                  <PackCard
                    pack={pack}
                    channelActive={channelActive}
                    sortedVersions={sortedVersions}
                    isActive={isActive}
                    onUpload={() => openUpload(pack.packId)}
                    onActivate={(v) => setActivate({ pack, v })}
                    onDeactivate={(v) => setDel({ pack, v })}
                    onStats={() => openStats(pack)}
                    onSetActive={(ch, ver) => {
                      const v = pack.versions.find((x) => x.version === ver && x.channel === ch);
                      if (v) setActivate({ pack, v });
                    }}
                  />
                )}
              </For>
            </div>
          </Show>
        </Show>
      </Show>

      {/* ===== 上传弹窗 ===== */}
      <Modal
        open={drawerOpen()}
        onClose={closeUpload}
        title="上传新版资源包"
        size="md"
        footer={
          <>
            <Btn variant="ghost" onClick={closeUpload} disabled={uploading()}>
              取消
            </Btn>
            <Btn
              variant="primary"
              icon="check"
              onClick={doUpload}
              disabled={uploading() || !upPackId().trim() || !upVersion().trim() || !upFile()}
            >
              {uploading() ? '上传中…' : '上传并签名'}
            </Btn>
          </>
        }
      >
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
          <div class="muted-3 mono" style={sx({ fontSize: 11 })}>
            POST /api/admin/resource-packs/:pack_id/versions
          </div>

          <Field label="资源包 ID (pack_id) *" hint="snake_case · 不可改">
            <input
              class="input mono"
              list="rp-pack-options"
              placeholder="例 wordbook-en-gre / amas-presets-default"
              value={upPackId()}
              disabled={uploading()}
              onInput={(e) => setUpPackId(e.currentTarget.value)}
            />
            <datalist id="rp-pack-options">
              <For each={allPacks()}>{(e) => <option value={e.packId} />}</For>
            </datalist>
          </Field>

          <Field label="版本号 (semver) *" hint="例 3.3.0-beta.2 / 1.0.0">
            <input
              class="input mono"
              placeholder="major.minor.patch[-pre]"
              value={upVersion()}
              disabled={uploading()}
              onInput={(e) => setUpVersion(e.currentTarget.value)}
            />
          </Field>

          <Field label="通道 *" hint="stable / beta / internal">
            <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: 8 })}>
              <For each={CHANNELS}>
                {(ch) => (
                  <button
                    type="button"
                    onClick={() => !uploading() && setUpChannel(ch)}
                    style={sx({
                      textAlign: 'left',
                      padding: '10px 12px',
                      borderRadius: 11,
                      cursor: uploading() ? 'default' : 'pointer',
                      border: `1px solid ${upChannel() === ch ? CH_COLOR[ch] : 'var(--border)'}`,
                      background: upChannel() === ch ? `color-mix(in oklch, ${CH_COLOR[ch]} 12%, transparent)` : 'var(--surface-sunken)',
                    })}
                  >
                    <div style={sx({ fontSize: 13, fontWeight: 700, color: upChannel() === ch ? CH_COLOR[ch] : 'var(--text)' })}>
                      {CH_META[ch].label}
                    </div>
                    <div class="muted-3" style={sx({ fontSize: 11, marginTop: 2 })}>
                      {CH_META[ch].desc}
                    </div>
                  </button>
                )}
              </For>
            </div>
          </Field>

          <Field label="最低 app 版本 (minAppVersion)" hint="可选">
            <input
              class="input mono"
              placeholder="例 0.7.0"
              value={upMinApp()}
              disabled={uploading()}
              onInput={(e) => setUpMinApp(e.currentTarget.value)}
            />
          </Field>

          <Field label="说明 (description)" hint="可选 · 仅给 admin 看">
            <textarea
              class="textarea"
              placeholder="例：补全 GRE 高频词 v2 列表 1,028 条 + 修正 17 个释义"
              value={upDesc()}
              disabled={uploading()}
              onInput={(e) => setUpDesc(e.currentTarget.value)}
            />
          </Field>

          {/* 拖拽区 */}
          <div
            onDragOver={(e) => {
              e.preventDefault();
              setDragOver(true);
            }}
            onDragLeave={() => setDragOver(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragOver(false);
              if (e.dataTransfer?.files.length) pickFile(e.dataTransfer.files[0]);
            }}
            style={sx({
              padding: 24,
              borderRadius: 12,
              border: `2px dashed ${dragOver() ? 'var(--accent)' : 'var(--border)'}`,
              background: dragOver() ? 'var(--accent-soft)' : 'var(--surface-sunken)',
              textAlign: 'center',
              color: 'var(--text-3)',
            })}
          >
            <Icon name="upload" size={26} />
            <div style={sx({ fontSize: 13, marginTop: 8, color: 'var(--text-2)', fontWeight: 600 })}>
              {upFile() ? upFile()!.name : '拖入 payload.json 或点击选择'}
            </div>
            <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 4 })}>
              {upFile()
                ? `${fmtBytes(upFile()!.size)} · ${upFile()!.type || 'application/json'}`
                : 'application/json · 单包硬上限 4 MiB · raw body 上传'}
            </div>
            <div style={sx({ marginTop: 10 })}>
              <Btn size="sm" variant="outline" disabled={uploading()} onClick={() => fileInput?.click()}>
                选择文件
              </Btn>
            </div>
            <input
              ref={fileInput}
              type="file"
              accept="application/json,.json"
              hidden
              onChange={(e) => pickFile(e.currentTarget.files?.[0] ?? null)}
            />
          </div>

          {/* 签名行 */}
          <div
            style={sx({
              display: 'grid',
              gridTemplateColumns: 'auto 1fr',
              gap: '6px 12px',
              padding: 12,
              borderRadius: 10,
              background: 'var(--surface-sunken)',
              border: '1px solid var(--hairline)',
              fontSize: 11.5,
            })}
          >
            <span class="muted-3">SHA-256</span>
            <span class="mono">{upSha() ? `${upSha()!.slice(0, 16)} ··· ${upSha()!.slice(-8)}` : '— 等待文件 —'}</span>
            <span class="muted-3">Ed25519 签名</span>
            <span style={sx({ color: 'var(--success)' })}>上传时由服务端签</span>
            <span class="muted-3">大小</span>
            <span class="mono">{upFile() ? fmtBytes(upFile()!.size) : '—'}</span>
          </div>

          <Show when={uploading()}>
            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 6 })}>
              <div style={sx({ display: 'flex', justifyContent: 'space-between', fontSize: 12 })}>
                <span class="muted">正在上传…</span>
                <strong class="mono">{progress()}%</strong>
              </div>
              <Progress value={progress()} tone="accent" height={8} />
              <div class="muted-3" style={sx({ fontSize: 11 })}>
                raw body → 服务端 SHA-256 + Ed25519 签名 + 落盘
              </div>
            </div>
          </Show>

          <div
            style={sx({
              padding: '10px 12px',
              borderRadius: 10,
              background: 'var(--warning-soft)',
              color: 'var(--warning)',
              fontSize: 11.5,
            })}
          >
            ⚠ 默认上传后不会自动激活，需手动切换通道激活版本。
          </div>
        </div>
      </Modal>

      {/* ===== 激活确认弹窗 ===== */}
      <Modal
        open={!!activate()}
        onClose={() => !acting() && setActivate(null)}
        title="切换激活版本？"
        size="md"
        footer={
          <>
            <Btn variant="ghost" onClick={() => setActivate(null)} disabled={acting()}>
              取消
            </Btn>
            <Btn variant="primary" icon="check" onClick={doActivate} disabled={acting()}>
              {acting() ? '广播中…' : '切换并广播'}
            </Btn>
          </>
        }
      >
        <Show when={activate()}>
          {(t) => (
            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
              <p class="muted" style={sx({ margin: 0, fontSize: 13, lineHeight: 1.6 })}>
                切换激活版本会通过 SSE 立即广播给所有在线客户端。客户端会在下次下载时拿到新版本，老版本仍保留在
                <span class="mono"> static/packs/ </span>
                中可回滚。
              </p>
              <div
                style={sx({
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 10,
                  padding: 14,
                  borderRadius: 12,
                  background: 'var(--surface-sunken)',
                })}
              >
                <DiffRow label="当前激活">
                  <Show when={activeCurrent()} fallback={<span class="muted-3">无</span>}>
                    {(cur) => (
                      <span class="mono">
                        v{cur().version} · <span class="muted-3">{fmtAgo(cur().publishedAt)}</span>
                      </span>
                    )}
                  </Show>
                </DiffRow>
                <DiffRow label="即将激活">
                  <span class="mono" style={sx({ color: 'var(--accent)', fontWeight: 700 })}>
                    v{t().v.version} · <span class="muted-3">{fmtAgo(t().v.publishedAt)}</span>
                  </span>
                </DiffRow>
                <DiffRow label="通道">
                  <Badge variant={CH_META[t().v.channel].badge}>{CH_META[t().v.channel].label}</Badge>
                </DiffRow>
                <DiffRow label="受众">
                  <span>
                    在线客户端 ~ <strong>{summary()?.onlineClients ?? '—'}</strong> 个
                  </span>
                </DiffRow>
              </div>
              <div
                style={sx({
                  padding: '10px 12px',
                  borderRadius: 10,
                  background: 'var(--info-soft)',
                  color: 'var(--text-2)',
                  fontSize: 11.5,
                  lineHeight: 1.6,
                })}
              >
                <strong>SSE 通告范围：</strong>所有订阅了 {CH_META[t().v.channel].label} 通道的在线客户端
                <Show when={summary()}>{(s) => <> (~{s().onlineClients})</>}</Show>。5 分钟内重复切换会被服务端去重，不会重复推送。
              </div>
            </div>
          )}
        </Show>
      </Modal>

      {/* ===== 停用确认弹窗 ===== */}
      <Confirm
        open={!!del()}
        onClose={() => setDel(null)}
        onConfirm={doDeactivate}
        loading={acting()}
        danger
        title="确认停用版本？"
        confirmText="确认停用"
        body={
          <Show when={del()}>
            {(t) => (
              <span>
                停用 <span class="mono">{t().v.version}</span>{' '}
                后将从客户端 manifest 中摘除（已下载客户端不受影响，软删除，文件保留可恢复）。
              </span>
            )}
          </Show>
        }
      />

      {/* ===== 安装统计弹窗 ===== */}
      <Modal
        open={!!statsPack()}
        onClose={() => setStatsPack(null)}
        title="安装统计"
        size="lg"
        footer={
          <Btn variant="ghost" onClick={() => setStatsPack(null)}>
            关闭
          </Btn>
        }
      >
        <Show when={statsPack()}>
          {(p) => (
            <div>
              <div class="muted-3 mono" style={sx({ fontSize: 11, marginBottom: 12 })}>
                {p().packId} · GET /:packId/stats
              </div>
              <Show when={statsRows()} fallback={<Loading h={140} />}>
                {(rows) => {
                  const list = aggregateStats(rows());
                  return (
                    <Show
                      when={list.length > 0}
                      fallback={<Empty title="暂无安装记录" desc="该资源包暂无 install_log 记录" icon="chart" />}
                    >
                      <div style={sx({ overflowX: 'auto' })}>
                        <table class="tbl">
                          <thead>
                            <tr>
                              <th>版本</th>
                              <th style={sx({ textAlign: 'right' })}>installed</th>
                              <th style={sx({ textAlign: 'right' })}>verify_failed</th>
                              <th style={sx({ textAlign: 'right' })}>rollback</th>
                              <th style={sx({ textAlign: 'right' })}>合计</th>
                            </tr>
                          </thead>
                          <tbody>
                            <For each={list}>
                              {(r) => (
                                <tr>
                                  <td class="mono">{r.version}</td>
                                  <td class="mono" style={sx({ textAlign: 'right', color: 'var(--success)' })}>
                                    {fmtNum(r.installed)}
                                  </td>
                                  <td class="mono" style={sx({ textAlign: 'right', color: 'var(--warning)' })}>
                                    {fmtNum(r.verify_failed)}
                                  </td>
                                  <td class="mono" style={sx({ textAlign: 'right', color: 'var(--error)' })}>
                                    {fmtNum(r.rollback)}
                                  </td>
                                  <td class="mono" style={sx({ textAlign: 'right', fontWeight: 700 })}>
                                    {fmtNum(r.installed + r.verify_failed + r.rollback)}
                                  </td>
                                </tr>
                              )}
                            </For>
                          </tbody>
                        </table>
                      </div>
                    </Show>
                  );
                }}
              </Show>
            </div>
          )}
        </Show>
      </Modal>
    </div>
  );
}

/* ---------------- 局部组件 ---------------- */

function DiffRow(props: { label: string; children: JSX.Element }) {
  return (
    <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 })}>
      <span class="muted-3" style={sx({ fontSize: 12 })}>
        {props.label}
      </span>
      <span style={sx({ fontSize: 13 })}>{props.children}</span>
    </div>
  );
}

type StatAgg = { version: string; installed: number; verify_failed: number; rollback: number };

function aggregateStats(rows: PackStatsEntry[]): StatAgg[] {
  const byVer = new Map<string, StatAgg>();
  for (const r of rows) {
    const a = byVer.get(r.version) ?? { version: r.version, installed: 0, verify_failed: 0, rollback: 0 };
    if (r.outcome === 'installed') a.installed += r.count;
    else if (r.outcome === 'verify_failed') a.verify_failed += r.count;
    else if (r.outcome === 'rollback') a.rollback += r.count;
    byVer.set(r.version, a);
  }
  return Array.from(byVer.values()).sort((x, y) => (y.version > x.version ? 1 : -1));
}

function PackCard(props: {
  pack: AdminPackEntry;
  channelActive: (pack: AdminPackEntry, ch: PackChannel) => ResourcePackVersion | null;
  sortedVersions: (pack: AdminPackEntry) => ResourcePackVersion[];
  isActive: (pack: AdminPackEntry, v: ResourcePackVersion) => boolean;
  onUpload: () => void;
  onActivate: (v: ResourcePackVersion) => void;
  onDeactivate: (v: ResourcePackVersion) => void;
  onStats: () => void;
  onSetActive: (ch: PackChannel, ver: string) => void;
}) {
  const pack = () => props.pack;

  // 每个通道可激活的候选版本（同通道、未停用）
  const candidates = (ch: PackChannel) =>
    pack()
      .versions.filter((v) => v.channel === ch && !v.deactivatedAt)
      .sort((a, b) => (b.publishedAt > a.publishedAt ? 1 : -1));

  // 安装统计柱状图（按版本 totalInstalls 近似：用 outcomes 无法分版本，这里用版本表的近 7 天 installed 不可得，
  // 故用各版本签名状态展示不直观；改用 outcomes7d 三态条形对比，符合真实可得字段）
  const outcomeBars = createMemo(() => {
    const o = pack().outcomes7d;
    return [
      { label: 'installed', value: o.installed, color: 'var(--success)' },
      { label: 'verify_failed', value: o.verify_failed, color: 'var(--warning)' },
      { label: 'rollback', value: o.rollback, color: 'var(--error)' },
    ];
  });

  return (
    <Card>
      {/* 头部 */}
      <div
        style={sx({
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'flex-start',
          flexWrap: 'wrap',
          gap: 12,
          marginBottom: 14,
        })}
      >
        <div style={sx({ display: 'flex', gap: 12, alignItems: 'center', minWidth: 0 })}>
          <span
            style={sx({
              width: 40,
              height: 40,
              borderRadius: 11,
              display: 'grid',
              placeItems: 'center',
              background: 'var(--accent-soft)',
              color: 'var(--accent)',
              flex: 'none',
            })}
          >
            <Icon name="package" size={20} />
          </span>
          <div style={sx({ minWidth: 0 })}>
            <div style={sx({ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' })}>
              <b style={sx({ fontSize: 15 })}>{pack().description || pack().packId}</b>
              <Badge variant="default">{pack().versions.length} 版本</Badge>
            </div>
            <div class="muted-3 mono" style={sx({ fontSize: 11.5, marginTop: 2 })}>
              {pack().packId} · {fmtNum(pack().totalInstalls)} 总下载
            </div>
          </div>
        </div>
        <div style={sx({ display: 'flex', gap: 8, alignItems: 'center' })}>
          <Btn size="sm" variant="ghost" icon="chart" onClick={props.onStats}>
            统计
          </Btn>
          <Btn size="sm" variant="secondary" icon="upload" onClick={props.onUpload}>
            上传新版
          </Btn>
        </div>
      </div>

      {/* 三通道激活选择器 */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: 10, marginBottom: 14 })}
      >
        <For each={CHANNELS}>
          {(ch) => {
            const av = () => props.channelActive(pack(), ch);
            const cands = () => candidates(ch);
            return (
              <div
                style={sx({
                  padding: 12,
                  borderRadius: 11,
                  border: `1px solid ${av() ? CH_COLOR[ch] : 'var(--hairline)'}`,
                  background: 'var(--surface-sunken)',
                })}
              >
                <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 })}>
                  <Badge variant={CH_META[ch].badge} dot>
                    {CH_META[ch].label}
                  </Badge>
                  <span class="muted-3" style={sx({ fontSize: 11 })}>
                    {cands().length} 可选
                  </span>
                </div>
                <Show
                  when={cands().length > 0}
                  fallback={
                    <div class="muted-3" style={sx({ fontSize: 12, padding: '4px 0' })}>
                      未发布
                    </div>
                  }
                >
                  <select
                    class="select mono"
                    style={sx({ fontSize: 12 })}
                    value={av()?.version ?? ''}
                    onChange={(e) => {
                      const ver = e.currentTarget.value;
                      if (ver && ver !== av()?.version) props.onSetActive(ch, ver);
                    }}
                  >
                    <Show when={!av()}>
                      <option value="">— 未激活 —</option>
                    </Show>
                    <For each={cands()}>
                      {(v) => (
                        <option value={v.version}>
                          v{v.version}
                          {av()?.version === v.version ? '（激活中）' : ''}
                        </option>
                      )}
                    </For>
                  </select>
                  <Show when={av()}>
                    {(v) => (
                      <div class="muted-3" style={sx({ fontSize: 10.5, marginTop: 6 })}>
                        {fmtBytes(v().sizeBytes)} · {fmtAgo(v().publishedAt)}
                      </div>
                    )}
                  </Show>
                </Show>
              </div>
            );
          }}
        </For>
      </div>

      {/* 版本表 */}
      <div style={sx({ overflowX: 'auto' })}>
        <table class="tbl">
          <thead>
            <tr>
              <th>版本</th>
              <th>通道</th>
              <th>大小</th>
              <th>SHA-256</th>
              <th>签名</th>
              <th>min app</th>
              <th>发布时间</th>
              <th>状态</th>
              <th style={sx({ textAlign: 'right' })}>操作</th>
            </tr>
          </thead>
          <tbody>
            <For each={props.sortedVersions(pack())}>
              {(v) => {
                const active = props.isActive(pack(), v);
                const replaced = !v.deactivatedAt && !active;
                return (
                  <tr>
                    <td class="mono">{v.version}</td>
                    <td>
                      <Badge variant={CH_META[v.channel].badge}>{CH_META[v.channel].label}</Badge>
                    </td>
                    <td class="mono">{fmtBytes(v.sizeBytes)}</td>
                    <td>
                      <button
                        class="mono"
                        title="复制 SHA-256"
                        onClick={() => copySha(v.sha256)}
                        style={sx({
                          border: 'none',
                          background: 'transparent',
                          cursor: 'pointer',
                          color: 'var(--text-3)',
                          fontSize: 11,
                          display: 'inline-flex',
                          gap: 5,
                          alignItems: 'center',
                        })}
                      >
                        {shaShort(v.sha256)} <Icon name="copy" size={11} />
                      </button>
                    </td>
                    <td>
                      <Show
                        when={v.signature}
                        fallback={
                          <span class="muted-3" style={sx({ fontSize: 11.5 })}>
                            未签名
                          </span>
                        }
                      >
                        <Badge variant="success" dot>
                          {v.signatureAlg}
                        </Badge>
                      </Show>
                    </td>
                    <td class="mono muted-3" style={sx({ fontSize: 11.5 })}>
                      {v.minAppVersion ? `≥ ${v.minAppVersion}` : '—'}
                    </td>
                    <td class="muted-3" style={sx({ fontSize: 11.5 })}>
                      {fmtAgo(v.publishedAt)}
                    </td>
                    <td>
                      <Show
                        when={active}
                        fallback={
                          <Show
                            when={v.deactivatedAt}
                            fallback={
                              <Badge variant="default" dot>
                                已被替换
                              </Badge>
                            }
                          >
                            <Badge variant="error" dot>
                              已停用
                            </Badge>
                          </Show>
                        }
                      >
                        <Badge variant="success" dot>
                          激活中
                        </Badge>
                      </Show>
                    </td>
                    <td>
                      <div style={sx({ display: 'flex', gap: 5, justifyContent: 'flex-end' })}>
                        <Show when={replaced}>
                          <Btn size="xs" variant="primary" onClick={() => props.onActivate(v)}>
                            激活
                          </Btn>
                        </Show>
                        <Show when={!v.deactivatedAt}>
                          <Btn size="xs" variant="outline" onClick={() => props.onDeactivate(v)}>
                            停用
                          </Btn>
                        </Show>
                      </div>
                    </td>
                  </tr>
                );
              }}
            </For>
          </tbody>
        </table>
      </div>

      {/* 近 7 天安装结果 */}
      <div style={sx({ marginTop: 14 })}>
        <div class="eyebrow" style={sx({ marginBottom: 8 })}>
          近 7 天安装结果 · install_log 聚合
        </div>
        <BarChart horizontal data={outcomeBars()} fmtV={(n) => fmtNum(n)} />
      </div>
    </Card>
  );
}
