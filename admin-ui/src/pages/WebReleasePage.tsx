import { createResource, createSignal, createMemo, For, Show, type JSX } from 'solid-js';
import {
  Btn,
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
  Icon,
  PageHead,
  sx,
  toast,
  fmtNum,
  fmtBytes,
  fmtAgo,
} from '@/components/wf';
import { packsApi, type AdminPackEntry, type ResourcePackVersion, type PackSummary } from '@/api/packs';
import { statusApi, type AppStatus } from '@/api/status';

/**
 * Web 发版页。
 * 把 wordforge-web 构建（dist 的 tar.gz）作为 packId=web-app 包上传 + Ed25519 签名，
 * 发布后服务端解包并原子切换站点根 static/web-app/current，经 SSE 通知在线客户端热更新。
 * 仅 stable 通道驱动线上托管 · 单工件上限 128 MiB。
 * 区别于「版本更新」（后端二进制自更新）与「资源包」（JSON 内容包）。
 */

const WEB_APP_PACK_ID = 'web-app';
const MAX_UPLOAD = 128 * 1024 * 1024; // 后端 web-app 上限
const SHA_PREVIEW_LIMIT = 32 * 1024 * 1024; // 超过则跳过本地 SHA-256 预览（同步计算会卡 UI，服务端总会重算）

const shaShort = (h: string) => `${h.slice(0, 12)} … ${h.slice(-6)}`;

function copySha(h: string) {
  navigator.clipboard?.writeText(h).then(
    () => toast.success('已复制 SHA-256'),
    () => {},
  );
}

export default function WebReleasePage() {
  const [status, { refetch: refetchStatus }] = createResource<AppStatus | null>(() =>
    statusApi.get().catch(() => null),
  );
  const [entries, { refetch: refetchEntries }] = createResource<AdminPackEntry[]>(() => packsApi.list());
  const [summary, { refetch: refetchSummary }] = createResource<PackSummary | null>(() =>
    packsApi.summary().catch(() => null),
  );

  // —— 上传弹窗 ——
  const [uploadOpen, setUploadOpen] = createSignal(false);
  const [upVersion, setUpVersion] = createSignal('');
  const [upMinApp, setUpMinApp] = createSignal('');
  const [upDesc, setUpDesc] = createSignal('');
  const [upFile, setUpFile] = createSignal<File | null>(null);
  const [upSha, setUpSha] = createSignal<string | null>(null);
  const [shaSkipped, setShaSkipped] = createSignal(false);
  const [dragOver, setDragOver] = createSignal(false);
  const [uploading, setUploading] = createSignal(false);
  const [progress, setProgress] = createSignal(0);
  let fileInput: HTMLInputElement | undefined;

  // —— 发布 / 停用 ——
  const [publishTarget, setPublishTarget] = createSignal<ResourcePackVersion | null>(null);
  const [delTarget, setDelTarget] = createSignal<ResourcePackVersion | null>(null);
  const [acting, setActing] = createSignal(false);

  const webApp = createMemo(() => entries()?.find((e) => e.packId === WEB_APP_PACK_ID) ?? null);
  // 线上版本：优先后端 status.webTargetVersion（真实托管版本），回退到列表 active.stable
  const liveVersion = createMemo(() => status()?.webTargetVersion ?? webApp()?.active.stable ?? null);

  const allVersions = createMemo(() =>
    [...(webApp()?.versions ?? [])].sort((a, b) => b.publishedAt.localeCompare(a.publishedAt)),
  );
  const stableVersions = createMemo(() => allVersions().filter((v) => v.channel === 'stable'));
  const liveRow = createMemo(() => stableVersions().find((v) => v.version === liveVersion()) ?? null);

  const isLive = (v: ResourcePackVersion) =>
    liveVersion() === v.version && v.channel === 'stable' && !v.deactivatedAt;
  // 目标比当前线上更旧 → 视为回滚
  const isRollback = (v: ResourcePackVersion) => {
    const cur = liveRow();
    return !!cur && v.publishedAt < cur.publishedAt;
  };

  const fileNameOk = createMemo(() => {
    const f = upFile();
    return !f || /\.(tar\.gz|tgz)$/i.test(f.name);
  });

  async function refreshAll() {
    await Promise.all([refetchStatus(), refetchEntries(), refetchSummary()]);
  }

  // —— 上传 ——
  async function pickFile(f: File | null) {
    setUpSha(null);
    setShaSkipped(false);
    if (!f) {
      setUpFile(null);
      return;
    }
    if (f.size > MAX_UPLOAD) {
      toast.warning('文件超过 128 MiB 上限');
      setUpFile(null);
      return;
    }
    setUpFile(f);
    if (f.size > SHA_PREVIEW_LIMIT) {
      setShaSkipped(true); // 大文件跳过本地预览，服务端会重算
      return;
    }
    try {
      const buf = await f.arrayBuffer();
      const hash = await crypto.subtle.digest('SHA-256', buf);
      setUpSha(
        Array.from(new Uint8Array(hash))
          .map((b) => b.toString(16).padStart(2, '0'))
          .join(''),
      );
    } catch {
      setUpSha(null);
    }
  }

  function openUpload() {
    setUpVersion('');
    setUpMinApp('');
    setUpDesc('');
    setUpFile(null);
    setUpSha(null);
    setShaSkipped(false);
    setProgress(0);
    setUploadOpen(true);
  }
  function closeUpload() {
    if (!uploading()) setUploadOpen(false);
  }

  async function doUpload() {
    if (!upVersion().trim() || !upFile()) {
      toast.warning('请填写版本号并选择 tar.gz 文件');
      return;
    }
    setUploading(true);
    setProgress(0);
    try {
      await packsApi.uploadVersion(
        WEB_APP_PACK_ID,
        {
          version: upVersion().trim(),
          channel: 'stable',
          artifactType: 'tarball',
          minAppVersion: upMinApp().trim() || undefined,
          description: upDesc().trim() || undefined,
        },
        upFile()!,
        (frac) => setProgress(Math.round(frac * 100)),
      );
      toast.success('上传成功', '已 SHA-256 + Ed25519 签名落盘；尚未发布，请在下方版本列表点「发布」');
      setUploadOpen(false);
      await refreshAll();
    } catch (err) {
      const msg = err instanceof Error ? err.message : '';
      if (/EXIST/i.test(msg)) toast.error('该版本号已存在', '版本不可覆盖，请改用新版本号');
      else if (/TOO_LARGE|413/i.test(msg)) toast.error('文件超过 128 MiB 上限', msg);
      else if (/SIGNER_UNAVAILABLE|503/i.test(msg)) toast.error('签名器不可用', '服务端 Ed25519 签名器未初始化');
      else toast.error('上传失败', msg);
    } finally {
      setUploading(false);
    }
  }

  // —— 发布 / 回滚 ——
  async function doPublish() {
    const t = publishTarget();
    if (!t) return;
    setActing(true);
    try {
      const res = await packsApi.setActive(WEB_APP_PACK_ID, 'stable', t.version);
      toast.success(
        `${isRollback(t) ? '已回滚' : '已发布'} web v${t.version} 到线上`,
        `经 SSE 通知 ~${fmtNum(res.audienceClients)} 个在线客户端，将按策略热更新`,
      );
      setPublishTarget(null);
      await refreshAll();
    } catch (err) {
      const msg = err instanceof Error ? err.message : '';
      if (/CHANNEL_MISMATCH/i.test(msg)) toast.error('通道不匹配', '该版本非 stable 通道，无法发布到线上');
      else if (/DEACTIVATED/i.test(msg)) toast.error('版本已停用', '请选择未停用的版本');
      else if (/NOT_TARBALL/i.test(msg)) toast.error('工件非 tar.gz', '该版本不是 web 构建，无法发布');
      else if (/NOT_FOUND|404/i.test(msg)) toast.error('版本不存在', msg);
      else toast.error('发布失败', msg);
    } finally {
      setActing(false);
    }
  }

  // —— 停用 ——
  async function doDeactivate() {
    const t = delTarget();
    if (!t) return;
    setActing(true);
    try {
      await packsApi.deactivateVersion(WEB_APP_PACK_ID, t.version);
      toast.success('已停用版本', '从可发布列表摘除，文件保留可恢复');
      setDelTarget(null);
      await refreshAll();
    } catch (err) {
      toast.error('停用失败', err instanceof Error ? err.message : '');
    } finally {
      setActing(false);
    }
  }

  return (
    <div>
      <PageHead
        title="Web 发版"
        desc="把 wordforge-web 构建（dist 打包的 tar.gz）作为 web-app 包上传 + Ed25519 签名；发布后服务端解包并原子切换站点根托管，经 SSE 通知在线客户端热更新。仅 stable 通道 · 单工件上限 128 MiB。区别于「版本更新」（后端二进制）与「资源包」（JSON 内容包）。"
        right={
          <>
            <Btn variant="secondary" icon="refresh" onClick={refreshAll} disabled={entries.loading}>
              刷新
            </Btn>
            <Btn variant="primary" icon="upload" onClick={openUpload}>
              上传新版
            </Btn>
          </>
        }
      />

      {/* —— KPI —— */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'repeat(4, minmax(0,1fr))', gap: 14, marginBottom: 18 })}
      >
        <StatCard
          tone="accent"
          icon="upload"
          label="当前线上 web 版本"
          value={liveVersion() ? `v${liveVersion()}` : '未发布'}
          deltaLabel={liveRow() ? `发布于 ${fmtAgo(liveRow()!.publishedAt)}` : 'static/web-app/current'}
        />
        <StatCard
          tone="info"
          icon="zap"
          label="更新策略"
          value={status() ? (status()!.webPwaSilentUpdate ? '静默更新' : '提示刷新') : '—'}
          deltaLabel="webPwaSilentUpdate"
        />
        <StatCard
          tone="success"
          icon="devices"
          label="在线设备"
          value={fmtNum(summary()?.onlineClients ?? 0)}
          deltaLabel="发布即 SSE 通知"
        />
        <StatCard
          tone="warning"
          icon="package"
          label="已上传版本"
          value={stableVersions().length}
          deltaLabel={`共 ${allVersions().length} 条记录`}
        />
      </div>

      {/* —— 版本历史 —— */}
      <Panel title="版本历史" sub="按发布时间倒序 · 仅 stable 通道驱动线上托管">
        <Show when={!entries.loading} fallback={<Loading h={160} />}>
          <Show
            when={!entries.error}
            fallback={
              <Empty
                title="加载失败"
                desc={entries.error instanceof Error ? entries.error.message : '无法获取版本列表'}
                icon="alert"
                action={
                  <Btn variant="secondary" icon="refresh" onClick={refreshAll}>
                    重试
                  </Btn>
                }
              />
            }
          >
            <Show
              when={allVersions().length > 0}
              fallback={
                <Empty
                  title="尚无 web 版本"
                  desc="点击右上角「上传新版」上传第一个 web 构建（tar.gz），上传后再发布到线上"
                  icon="upload"
                  action={
                    <Btn variant="primary" icon="upload" onClick={openUpload}>
                      上传新版
                    </Btn>
                  }
                />
              }
            >
              <div style={sx({ overflowX: 'auto' })}>
                <table class="tbl">
                  <thead>
                    <tr>
                      <th>版本</th>
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
                    <For each={allVersions()}>
                      {(v) => (
                        <tr>
                          <td class="mono">
                            v{v.version}
                            <Show when={v.channel !== 'stable'}>
                              <Badge variant="warning">{v.channel}</Badge>
                            </Show>
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
                              when={isLive(v)}
                              fallback={
                                <Show
                                  when={v.deactivatedAt}
                                  fallback={
                                    <Badge variant="default" dot>
                                      历史
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
                                线上中
                              </Badge>
                            </Show>
                          </td>
                          <td>
                            <div style={sx({ display: 'flex', gap: 5, justifyContent: 'flex-end' })}>
                              <Show when={!isLive(v) && !v.deactivatedAt && v.channel === 'stable'}>
                                <Btn
                                  size="xs"
                                  variant={isRollback(v) ? 'warning' : 'primary'}
                                  icon={isRollback(v) ? 'rotate' : 'upload'}
                                  onClick={() => setPublishTarget(v)}
                                >
                                  {isRollback(v) ? '回滚' : '发布'}
                                </Btn>
                              </Show>
                              <Show when={!isLive(v) && !v.deactivatedAt}>
                                <Btn size="xs" variant="outline" icon="ban" onClick={() => setDelTarget(v)}>
                                  停用
                                </Btn>
                              </Show>
                            </div>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>
          </Show>
        </Show>
      </Panel>

      {/* ===== 上传弹窗 ===== */}
      <Modal
        open={uploadOpen()}
        onClose={closeUpload}
        title="上传新版 Web 构建"
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
              disabled={uploading() || !upVersion().trim() || !upFile()}
            >
              {uploading() ? '上传中…' : '上传并签名'}
            </Btn>
          </>
        }
      >
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
          <div class="muted-3 mono" style={sx({ fontSize: 11 })}>
            POST /api/admin/resource-packs/web-app/versions?channel=stable&amp;artifactType=tarball
          </div>

          <Field label="版本号 (semver) *" hint="例 3.3.0 / 3.3.0-beta.2 · 不可与已有版本重复">
            <input
              class="input mono"
              placeholder="major.minor.patch[-pre]"
              value={upVersion()}
              disabled={uploading()}
              onInput={(e) => setUpVersion(e.currentTarget.value)}
            />
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
              placeholder="例：修复登录页闪烁 + 升级依赖；对应 wordforge-web commit abc1234"
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
              {upFile() ? upFile()!.name : '拖入 dist 打的 *.tar.gz 或点击选择'}
            </div>
            <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 4 })}>
              {upFile()
                ? `${fmtBytes(upFile()!.size)} · ${upFile()!.type || 'application/gzip'}`
                : 'tar -czf web.tar.gz -C dist . · 须含 index.html · 上限 128 MiB'}
            </div>
            <div style={sx({ marginTop: 10 })}>
              <Btn size="sm" variant="outline" disabled={uploading()} onClick={() => fileInput?.click()}>
                选择文件
              </Btn>
            </div>
            <input
              ref={fileInput}
              type="file"
              accept=".tar.gz,.tgz,application/gzip,application/x-gzip"
              hidden
              onChange={(e) => pickFile(e.currentTarget.files?.[0] ?? null)}
            />
          </div>

          <Show when={upFile() && !fileNameOk()}>
            <div
              style={sx({
                padding: '8px 12px',
                borderRadius: 10,
                background: 'var(--warning-soft)',
                color: 'var(--warning)',
                fontSize: 11.5,
              })}
            >
              ⚠ 文件名不是 .tar.gz / .tgz，请确认这是 gzip 压缩的 tar 包；激活时若非 tar.gz 会被拒绝。
            </div>
          </Show>

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
            <span class="mono">
              {upSha()
                ? `${upSha()!.slice(0, 16)} ··· ${upSha()!.slice(-8)}`
                : shaSkipped()
                  ? '大文件，跳过本地预览（服务端重算）'
                  : '— 等待文件 —'}
            </span>
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
                raw body → 服务端 SHA-256 + Ed25519 签名 + 落盘 payload.tar.gz
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
            ⚠ 上传不会自动发布。上传成功后需在版本列表点「发布」才会切换线上托管。
          </div>
        </div>
      </Modal>

      {/* ===== 发布 / 回滚确认 ===== */}
      <Modal
        open={!!publishTarget()}
        onClose={() => !acting() && setPublishTarget(null)}
        title={publishTarget() && isRollback(publishTarget()!) ? '回滚到此版本？' : '发布到线上？'}
        size="md"
        footer={
          <>
            <Btn variant="ghost" onClick={() => setPublishTarget(null)} disabled={acting()}>
              取消
            </Btn>
            <Btn
              variant={publishTarget() && isRollback(publishTarget()!) ? 'warning' : 'primary'}
              icon="check"
              onClick={doPublish}
              disabled={acting()}
            >
              {acting() ? '发布中…' : publishTarget() && isRollback(publishTarget()!) ? '确认回滚' : '确认发布'}
            </Btn>
          </>
        }
      >
        <Show when={publishTarget()}>
          {(t) => (
            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
              <p class="muted" style={sx({ margin: 0, fontSize: 13, lineHeight: 1.6 })}>
                发布会让服务端校验签名后解包并原子切换站点根
                <span class="mono"> static/web-app/current </span>
                到此版本，并经 SSE 立即通知所有在线客户端按策略热更新。旧版本保留在磁盘，可随时回滚。
              </p>
              <div
                style={sx({ display: 'flex', flexDirection: 'column', gap: 10, padding: 14, borderRadius: 12, background: 'var(--surface-sunken)' })}
              >
                <DiffRow label="当前线上">
                  <Show when={liveVersion()} fallback={<span class="muted-3">未发布</span>}>
                    <span class="mono">v{liveVersion()}</span>
                  </Show>
                </DiffRow>
                <DiffRow label="即将发布">
                  <span class="mono" style={sx({ color: 'var(--accent)', fontWeight: 700 })}>
                    v{t().version} · <span class="muted-3">{fmtAgo(t().publishedAt)}</span>
                  </span>
                </DiffRow>
                <DiffRow label="大小">
                  <span class="mono">{fmtBytes(t().sizeBytes)}</span>
                </DiffRow>
                <DiffRow label="受众">
                  <span>
                    在线设备 ~ <strong>{fmtNum(summary()?.onlineClients ?? 0)}</strong> 个
                  </span>
                </DiffRow>
              </div>
              <div
                style={sx({ padding: '10px 12px', borderRadius: 10, background: 'var(--info-soft)', color: 'var(--text-2)', fontSize: 11.5, lineHeight: 1.6 })}
              >
                更新策略：
                <strong>{status()?.webPwaSilentUpdate ? '静默更新' : '提示用户刷新'}</strong>
                。5 分钟内重复发布会被服务端去重，不会重复推送。
              </div>
            </div>
          )}
        </Show>
      </Modal>

      {/* ===== 停用确认 ===== */}
      <Confirm
        open={!!delTarget()}
        onClose={() => setDelTarget(null)}
        onConfirm={doDeactivate}
        loading={acting()}
        danger
        title="确认停用版本？"
        confirmText="确认停用"
        body={
          <Show when={delTarget()}>
            {(t) => (
              <span>
                停用 <span class="mono">v{t().version}</span> 后将从可发布列表摘除（软删除，文件保留可恢复）。当前线上版本不受影响。
              </span>
            )}
          </Show>
        }
      />
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
