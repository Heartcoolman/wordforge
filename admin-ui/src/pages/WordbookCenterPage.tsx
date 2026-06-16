import { createSignal, createResource, createEffect, createMemo, Show, For, type JSX } from 'solid-js';
import {
  Btn, IconBtn, Badge, Card, Field, Tabs, Empty, Skel, Loading, Spinner,
  Modal, Confirm, Progress, PageHead, Icon, fmtNum, fmtAgo, sx, toast,
} from '@/components/wf';
import { adminApi } from '@/api/admin';
import type {
  WordbookSummary, WordbookType, WordbookSortKey, WordbookTypeFilter, WordEntrySort,
  WordbookListResponse, WordbookStats, WordEntriesPage, WordbookHeatmap,
  WordbookUserDistribution, WordbookAuditPage, WordbookExport,
} from '@/types/adminWordbooks';
import type { BrowseItem, UpdateInfo, WordbookPreview } from '@/types/wordbookCenter';

/* ---------- helpers (ported from handoff MetricBox / SimpleList) ---------- */

function MetricBox(props: { label: string; value: JSX.Element | string | number }) {
  return (
    <div style={sx({ padding: '11px 12px', borderRadius: 11, background: 'var(--surface-sunken)', border: '1px solid var(--hairline)' })}>
      <div class="muted-3" style={sx({ fontSize: 10.5 })}>{props.label}</div>
      <div class="tnum" style={sx({ fontSize: 18, fontWeight: 700, marginTop: 2 })}>{props.value}</div>
    </div>
  );
}

const ACTION_LABEL: Record<string, string> = {
  create: '创建', update: '编辑', delete: '删除', add_word: '添加词条',
  remove_word: '移除词条', import: '导入', sync: '同步',
};

const DETAIL_TABS = [
  { value: 'words', label: '词条' },
  { value: 'heatmap', label: '掌握热图' },
  { value: 'distrib', label: '用户分布' },
  { value: 'history', label: '变更记录' },
];

const PER_PAGE_WORDS = 30;

const CSV_TEMPLATE = `word,pos,ipa,gloss_zh,example
tenacious,adj.,/tɪˈneɪʃəs/,坚韧的,She was tenacious…
ephemeral,adj.,/ɪˈfɛmərəl/,短暂的,Fame can be…`;

/* ---------- page ---------- */

export default function WordbookCenterPage() {
  const [selected, setSelected] = createSignal<WordbookSummary | null>(null);
  const [tab, setTab] = createSignal<string>('words');
  const [search, setSearch] = createSignal('');
  const [typeFilter, setTypeFilter] = createSignal<WordbookTypeFilter>('all');
  const [sort, setSort] = createSignal<WordbookSortKey>('hottest');

  const [createOpen, setCreateOpen] = createSignal(false);
  const [editOpen, setEditOpen] = createSignal(false);
  const [delTarget, setDelTarget] = createSignal<WordbookSummary | null>(null);
  const [delBusy, setDelBusy] = createSignal(false);
  const [showRemote, setShowRemote] = createSignal(false);

  const [list, { refetch: reloadList }] = createResource(
    () => ({ search: search().trim(), type: typeFilter(), sort: sort() }),
    (q): Promise<WordbookListResponse> =>
      adminApi.adminWordbooksList({ ...q, perPage: 200 }).catch(() => ({
        items: [], total: 0, page: 1, perPage: 200, totalPages: 1,
        counts: { all: 0, system: 0, user: 0, totalEntries: 0 },
      })),
  );

  const items = () => list()?.items ?? [];
  const counts = () => list()?.counts;

  // 列表加载后自动选中第一项;选中项随刷新保持引用同步
  createEffect(() => {
    const arr = items();
    const cur = selected();
    if (arr.length === 0) { if (cur) setSelected(null); return; }
    if (!cur) { setSelected(arr[0]); return; }
    const fresh = arr.find((w) => w.id === cur.id);
    if (fresh) { if (fresh !== cur) setSelected(fresh); }
    else setSelected(arr[0]);
  });

  function selectWb(wb: WordbookSummary) {
    if (selected()?.id === wb.id) { setSelected(wb); return; }
    setSelected(wb);
    setTab('words');
  }

  async function confirmDelete() {
    const wb = delTarget();
    if (!wb) return;
    setDelBusy(true);
    try {
      await adminApi.adminWordbookDelete(wb.id);
      toast.success('已删除词库', wb.name);
      if (selected()?.id === wb.id) setSelected(null);
      setDelTarget(null);
      await reloadList();
    } catch (err: unknown) {
      toast.error('删除失败', err instanceof Error ? err.message : '');
    } finally {
      setDelBusy(false);
    }
  }

  return (
    <div>
      <PageHead
        title="词库中心"
        desc="管理系统与用户词库、词条、掌握度热图，导入远程词库目录或上传本地词库。"
        right={
          <>
            <Btn variant="secondary" icon="remote" onClick={() => setShowRemote((s) => !s)}>
              {showRemote() ? '隐藏远程目录' : '远程目录'}
            </Btn>
            <Btn variant="primary" icon="plus" onClick={() => setCreateOpen(true)}>新建词库</Btn>
          </>
        }
      />

      <Show when={showRemote()}>
        <RemoteCatalog onImported={() => { void reloadList(); }} />
      </Show>

      <div style={sx({ display: 'grid', gridTemplateColumns: '300px 1fr', gap: 16 })} class="grid-collapse">
        {/* LEFT — list */}
        <Card pad={false} style={sx({ overflow: 'hidden', alignSelf: 'start' })}>
          <div style={sx({ padding: '12px 14px', borderBottom: '1px solid var(--hairline)' })}>
            <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 })}>
              <span style={sx({ fontWeight: 700, fontSize: 13 })}>
                全部词库 <Show when={counts()}><span class="muted-3 mono" style={sx({ fontWeight: 400 })}>{counts()!.all}</span></Show>
              </span>
            </div>
            <div style={sx({ position: 'relative', marginTop: 10 })}>
              <span style={sx({ position: 'absolute', left: 9, top: '50%', transform: 'translateY(-50%)', color: 'var(--text-3)', display: 'grid', placeItems: 'center' })}>
                <Icon name="search" size={14} />
              </span>
              <input
                class="input"
                style={sx({ width: '100%', paddingLeft: 30 })}
                placeholder="搜索词库名 / 标签…"
                value={search()}
                onInput={(e) => setSearch(e.currentTarget.value)}
              />
            </div>
            <div style={sx({ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 10 })}>
              <FilterChip active={typeFilter() === 'all'} onClick={() => setTypeFilter('all')}>全部</FilterChip>
              <FilterChip active={typeFilter() === 'system'} onClick={() => setTypeFilter('system')}>官方</FilterChip>
              <FilterChip active={typeFilter() === 'user'} onClick={() => setTypeFilter('user')}>用户</FilterChip>
              <span style={sx({ width: 1, alignSelf: 'stretch', background: 'var(--hairline)', margin: '0 2px' })} />
              <FilterChip active={sort() === 'newest'} onClick={() => setSort('newest')}>最新</FilterChip>
              <FilterChip active={sort() === 'hottest'} onClick={() => setSort('hottest')}>最热</FilterChip>
            </div>
          </div>
          <div style={sx({ maxHeight: 560, overflowY: 'auto' })}>
            <Show
              when={list.state !== 'pending'}
              fallback={<div style={sx({ padding: 14 })}><Skel h={50} /><div style={sx({ height: 10 })} /><Skel h={50} /></div>}
            >
              <Show
                when={items().length > 0}
                fallback={<div style={sx({ padding: '8px 0' })}><Empty title="暂无词库" desc="点击右上「新建词库」创建,或从远程目录导入。" icon="book" /></div>}
              >
                <For each={items()}>
                  {(wb) => (
                    <button
                      onClick={() => selectWb(wb)}
                      style={sx({
                        width: '100%', textAlign: 'left', padding: '12px 14px', border: 'none',
                        borderBottom: '1px solid var(--hairline)', cursor: 'pointer',
                        background: selected()?.id === wb.id ? 'var(--accent-soft)' : 'transparent',
                      })}
                    >
                      <div style={sx({ display: 'flex', alignItems: 'center', gap: 7 })}>
                        <b style={sx({ fontSize: 13, color: selected()?.id === wb.id ? 'var(--accent)' : 'var(--text)' })}>{wb.name}</b>
                        <Show when={wb.type === 'system'} fallback={<Badge variant="default">用户</Badge>}>
                          <Badge variant="info">系统</Badge>
                        </Show>
                      </div>
                      <div class="muted-3" style={sx({ fontSize: 11, marginTop: 3 })}>
                        {fmtNum(wb.wordCount)} 词 · {fmtNum(wb.activeUsers)} 在用
                        <Show when={wb.type === 'user' && wb.ownerEmail}> · {wb.ownerEmail}</Show>
                      </div>
                    </button>
                  )}
                </For>
              </Show>
            </Show>
          </div>
        </Card>

        {/* RIGHT — detail */}
        <Show
          when={selected()}
          fallback={
            <Card style={sx({ display: 'grid', placeItems: 'center', minHeight: 320 })}>
              <Empty title="未选择词库" desc="从左侧选择一个词库查看详情。" icon="book" />
            </Card>
          }
        >
          {(wb) => (
            <WordbookDetail
              wb={wb()}
              tab={tab()}
              setTab={setTab}
              onEdit={() => setEditOpen(true)}
              onDelete={() => setDelTarget(wb())}
              onChanged={() => { void reloadList(); }}
            />
          )}
        </Show>
      </div>

      <CreateWordbookModal open={createOpen()} onClose={() => setCreateOpen(false)} onDone={() => { void reloadList(); }} />

      <EditWordbookModal
        open={editOpen()}
        wb={selected()}
        onClose={() => setEditOpen(false)}
        onDone={() => { void reloadList(); }}
      />

      <Confirm
        open={!!delTarget()}
        onClose={() => setDelTarget(null)}
        onConfirm={confirmDelete}
        danger
        loading={delBusy()}
        title="删除词库"
        confirmText="删除"
        body={
          <Show when={delTarget()}>
            {(t) => (
              <>
                确认删除「<b style={sx({ color: 'var(--text)' })}>{t().name}</b>」及其全部词条关联（{fmtNum(t().wordCount)} 词）？
                <br />该操作不可恢复。
              </>
            )}
          </Show>
        }
      />
    </div>
  );
}

function FilterChip(props: { active?: boolean; onClick: () => void; children: JSX.Element }) {
  return (
    <button
      onClick={props.onClick}
      class={props.active ? 'badge badge-accent' : 'badge badge-default'}
      style={sx({ cursor: 'pointer', border: 'none' })}
    >
      {props.children}
    </button>
  );
}

/* ---------- detail panel ---------- */

function WordbookDetail(props: {
  wb: WordbookSummary;
  tab: string;
  setTab: (v: string) => void;
  onEdit: () => void;
  onDelete: () => void;
  onChanged: () => void;
}) {
  const [search, setSearch] = createSignal('');
  const [wordSort, setWordSort] = createSignal<WordEntrySort>('frequency');
  const [page, setPage] = createSignal(1);
  const [addOpen, setAddOpen] = createSignal(false);
  const [removeTarget, setRemoveTarget] = createSignal<{ id: string; text: string } | null>(null);
  const [removeBusy, setRemoveBusy] = createSignal(false);
  const [exporting, setExporting] = createSignal(false);

  const wbId = () => props.wb.id;

  // 切换词库时重置词条 tab 状态
  createEffect(() => { wbId(); setSearch(''); setWordSort('frequency'); setPage(1); });

  const [stats] = createResource(wbId, (id): Promise<WordbookStats | null> =>
    adminApi.adminWordbookStats(id).catch(() => null));

  const [words, { refetch: reloadWords }] = createResource(
    () => (props.tab === 'words' ? { id: wbId(), s: wordSort(), p: page() } : null),
    (q): Promise<WordEntriesPage | null> =>
      adminApi.adminWordbookWords(q.id, { sort: q.s, page: q.p, perPage: PER_PAGE_WORDS }).catch(() => null),
  );

  const [heatmap] = createResource(
    () => (props.tab === 'heatmap' ? wbId() : null),
    (id): Promise<WordbookHeatmap | null> => adminApi.adminWordbookHeatmap(id).catch(() => null),
  );
  const [distrib] = createResource(
    () => (props.tab === 'distrib' ? wbId() : null),
    (id): Promise<WordbookUserDistribution | null> => adminApi.adminWordbookDistribution(id).catch(() => null),
  );
  const [history] = createResource(
    () => (props.tab === 'history' ? wbId() : null),
    (id): Promise<WordbookAuditPage | null> => adminApi.adminWordbookHistory(id, { perPage: 100 }).catch(() => null),
  );

  const rows = createMemo(() => {
    const all = words()?.data ?? [];
    const q = search().trim().toLowerCase();
    return q ? all.filter((w) => w.text.toLowerCase().includes(q) || w.meaning.toLowerCase().includes(q)) : all;
  });
  const totalPages = () => words()?.totalPages ?? 1;

  async function removeWord() {
    const t = removeTarget();
    if (!t) return;
    setRemoveBusy(true);
    try {
      await adminApi.adminWordbookRemoveWord(wbId(), t.id);
      toast.success('已移除', t.text);
      setRemoveTarget(null);
      await reloadWords();
      props.onChanged();
    } catch (err: unknown) {
      toast.error('移除失败', err instanceof Error ? err.message : '');
    } finally {
      setRemoveBusy(false);
    }
  }

  async function doExport() {
    setExporting(true);
    try {
      const data: WordbookExport = await adminApi.adminWordbookExport(wbId());
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${props.wb.name || props.wb.id}.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success('已导出词库', `${props.wb.name} · ${data.words.length} 词`);
    } catch (err: unknown) {
      toast.error('导出失败', err instanceof Error ? err.message : '');
    } finally {
      setExporting(false);
    }
  }

  const accColor = (acc?: number | null): string =>
    acc == null ? 'var(--text-3)' : acc >= 0.6 ? 'var(--success)' : acc >= 0.45 ? 'var(--warning)' : 'var(--error)';

  // 掌握度颜色:把 0..1 映射成 绿→红 渐变
  const masteryColor = (v: number): string =>
    `color-mix(in oklch, var(--success) ${Math.round(v * 100)}%, var(--error) ${Math.round((1 - v) * 60)}%)`;

  return (
    <Card pad={false} style={sx({ overflow: 'hidden' })}>
      {/* header */}
      <div style={sx({ padding: 16, borderBottom: '1px solid var(--hairline)' })}>
        <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 10, flexWrap: 'wrap' })}>
          <div style={sx({ minWidth: 0 })}>
            <div style={sx({ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' })}>
              <h2 style={sx({ margin: 0, fontSize: 17, fontWeight: 700 })}>{props.wb.name}</h2>
              <Show when={props.wb.type === 'system'} fallback={<Badge variant="default">用户</Badge>}>
                <Badge variant="info">系统</Badge>
              </Show>
              <For each={props.wb.tags}>{(t) => <Badge variant="default">{t}</Badge>}</For>
            </div>
            <Show when={props.wb.description}>
              <div class="muted" style={sx({ fontSize: 12.5, marginTop: 4 })}>{props.wb.description}</div>
            </Show>
          </div>
          <div style={sx({ display: 'flex', gap: 7, flexWrap: 'wrap' })}>
            <Btn size="sm" variant="secondary" icon="download" disabled={exporting()} onClick={doExport}>导出</Btn>
            <Btn size="sm" variant="secondary" icon="edit" onClick={props.onEdit}>编辑</Btn>
            <Btn size="sm" variant="danger" icon="trash" onClick={props.onDelete}>删除</Btn>
            <Btn size="sm" variant="primary" icon="plus" onClick={() => setAddOpen(true)}>添加词条</Btn>
          </div>
        </div>
        <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit,minmax(110px,1fr))', gap: 10, marginTop: 14 })}>
          <Show when={stats.state !== 'pending'} fallback={<><Skel h={56} /><Skel h={56} /><Skel h={56} /><Skel h={56} /></>}>
            <MetricBox label="词条数" value={fmtNum(stats()?.totalWords ?? props.wb.wordCount)} />
            <MetricBox label="在用用户" value={fmtNum(stats()?.activeUsers ?? props.wb.activeUsers)} />
            <MetricBox label="平均掌握" value={`${Math.round((stats()?.avgMastery ?? 0) * 100)}%`} />
            <MetricBox label="本周答题" value={fmtNum(stats()?.weeklyAnswers ?? 0)} />
          </Show>
        </div>
      </div>

      {/* tabs + word toolbar */}
      <div style={sx({ padding: '12px 16px', borderBottom: '1px solid var(--hairline)', display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' })}>
        <Tabs
          value={props.tab}
          onChange={props.setTab}
          tabs={DETAIL_TABS.map((t) => (t.value === 'words' ? { ...t, count: stats()?.totalWords ?? props.wb.wordCount } : t))}
        />
        <Show when={props.tab === 'words'}>
          <div style={sx({ display: 'flex', gap: 8, marginLeft: 'auto', flexWrap: 'wrap' })}>
            <input
              class="input"
              style={sx({ width: 160 })}
              placeholder="搜索词条"
              value={search()}
              onInput={(e) => setSearch(e.currentTarget.value)}
            />
            <select
              class="select"
              style={sx({ width: 'auto' })}
              value={wordSort()}
              onChange={(e) => { setWordSort(e.currentTarget.value as WordEntrySort); setPage(1); }}
            >
              <option value="frequency">排序：词频</option>
              <option value="alpha">排序：字母</option>
              <option value="difficulty">排序：难度</option>
            </select>
            <Btn size="sm" variant="primary" icon="plus" onClick={() => setAddOpen(true)}>添加词条</Btn>
          </div>
        </Show>
      </div>

      {/* body */}
      <div style={sx({ padding: 16 })}>
        {/* WORDS */}
        <Show when={props.tab === 'words'}>
          <Show when={words.state !== 'pending'} fallback={<Loading h={220} />}>
            <Show
              when={rows().length > 0}
              fallback={<Empty title="暂无词条" desc={search() ? '无匹配结果' : '点击「添加词条」录入第一个单词。'} icon="book" />}
            >
              <div style={sx({ overflowX: 'auto' })}>
                <table class="tbl">
                  <thead>
                    <tr><th>拼写</th><th>音标</th><th>词性</th><th>释义</th><th>答题数</th><th>正确率</th><th /></tr>
                  </thead>
                  <tbody>
                    <For each={rows()}>
                      {(w) => (
                        <tr>
                          <td><b>{w.text}</b></td>
                          <td class="mono muted">{w.pronunciation ?? '—'}</td>
                          <td class="muted">{w.partOfSpeech ?? '—'}</td>
                          <td class="muted" style={sx({ fontSize: 12.5 })}>{w.meaning || '—'}</td>
                          <td class="mono">{fmtNum(w.appearCount)}</td>
                          <td class="mono" style={sx({ color: accColor(w.accuracy) })}>
                            {w.accuracy == null ? '—' : `${Math.round(w.accuracy * 100)}%`}
                          </td>
                          <td><IconBtn name="trash" title="移除" size={15} onClick={() => setRemoveTarget({ id: w.id, text: w.text })} /></td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
              <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 14 })}>
                <span class="muted-3" style={sx({ fontSize: 12 })}>共 {fmtNum(words()?.total ?? 0)} 条</span>
                <div style={sx({ display: 'flex', alignItems: 'center', gap: 6 })}>
                  <Btn size="sm" variant="ghost" disabled={page() <= 1} onClick={() => setPage((p) => Math.max(1, p - 1))}>上一页</Btn>
                  <span class="mono muted" style={sx({ fontSize: 12.5, padding: '0 6px' })}>{page()} / {totalPages()}</span>
                  <Btn size="sm" variant="ghost" disabled={page() >= totalPages()} onClick={() => setPage((p) => Math.min(totalPages(), p + 1))}>下一页</Btn>
                </div>
              </div>
            </Show>
          </Show>
        </Show>

        {/* HEATMAP */}
        <Show when={props.tab === 'heatmap'}>
          <Show when={heatmap.state !== 'pending'} fallback={<Loading h={220} />}>
            <Show
              when={(heatmap()?.cells.length ?? 0) > 0}
              fallback={<Empty title="暂无答题数据" desc="该词库下的词暂无答题记录，无法生成热图。" icon="inbox" />}
            >
              <div class="muted-3" style={sx({ fontSize: 12, marginBottom: 10 })}>
                每格代表一个词条的答题频次（颜色越深答题越频繁）。
              </div>
              <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(40, 1fr)', gap: 2 })}>
                <For each={heatmap()!.cells.slice(0, 400)}>
                  {(cell) => {
                    const max = heatmap()!.maxCount || 1;
                    const r = Math.max(0, Math.min(1, cell.count / max));
                    return (
                      <div
                        title={`${cell.text} · ${fmtNum(cell.count)} 次`}
                        style={sx({
                          aspectRatio: '1', borderRadius: 2,
                          background: `color-mix(in oklch, var(--accent) ${Math.round(r * 90 + 6)}%, transparent)`,
                        })}
                      />
                    );
                  }}
                </For>
              </div>
              <div style={sx({ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginTop: 12 })}>
                <span class="muted-3" style={sx({ fontSize: 11 })}>显示前 {Math.min(400, heatmap()!.cells.length)} / {fmtNum(heatmap()!.cells.length)} 词 · 悬停查看单词</span>
                <div style={sx({ display: 'flex', alignItems: 'center', gap: 4 })}>
                  <span class="mono muted-3" style={sx({ fontSize: 10 })}>低</span>
                  <For each={[0.1, 0.3, 0.5, 0.7, 0.9]}>
                    {(r) => <span style={sx({ width: 14, height: 14, borderRadius: 3, background: `color-mix(in oklch, var(--accent) ${Math.round(r * 90 + 6)}%, transparent)` })} />}
                  </For>
                  <span class="mono muted-3" style={sx({ fontSize: 10 })}>高</span>
                </div>
              </div>
            </Show>
          </Show>
        </Show>

        {/* DISTRIBUTION */}
        <Show when={props.tab === 'distrib'}>
          <Show when={distrib.state !== 'pending'} fallback={<Loading h={220} />}>
            <Show
              when={(distrib()?.totalUsers ?? 0) > 0}
              fallback={<Empty title="暂无在用用户" desc="该词库下暂无用户掌握度数据。" icon="users" />}
            >
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
                <For each={distrib()!.buckets}>
                  {(b) => {
                    const total = distrib()!.totalUsers || 1;
                    const pct = (b.userCount / total) * 100;
                    return (
                      <div>
                        <div style={sx({ display: 'flex', justifyContent: 'space-between', fontSize: 13, marginBottom: 5 })}>
                          <span style={sx({ fontWeight: 600 })}>{b.label}</span>
                          <span class="mono">{fmtNum(b.userCount)} 人 · {pct.toFixed(0)}%</span>
                        </div>
                        <Progress value={pct} tone="accent" height={9} />
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>
          </Show>
        </Show>

        {/* HISTORY */}
        <Show when={props.tab === 'history'}>
          <Show when={history.state !== 'pending'} fallback={<Loading h={220} />}>
            <Show
              when={(history()?.data.length ?? 0) > 0}
              fallback={<Empty title="暂无变更记录" desc="对该词库的创建 / 编辑 / 导入等操作会记录在此。" icon="inbox" />}
            >
              <div style={sx({ display: 'flex', flexDirection: 'column' })}>
                <For each={history()!.data}>
                  {(h) => (
                    <div style={sx({ display: 'flex', alignItems: 'center', gap: 10, padding: '11px 0', borderBottom: '1px solid var(--hairline)' })}>
                      <div style={sx({ minWidth: 0, flex: 1 })}>
                        <div style={sx({ fontWeight: 600, fontSize: 13 })}>{h.detail || '—'}</div>
                        <div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 2 })}>
                          <Show when={h.adminId}>{h.adminId} · </Show>{fmtAgo(h.createdAt)}
                        </div>
                      </div>
                      <Badge variant="accent">{ACTION_LABEL[h.action] ?? h.action}</Badge>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </Show>
      </div>

      <AddWordModal open={addOpen()} onClose={() => setAddOpen(false)} wbId={wbId()} onDone={() => { void reloadWords(); props.onChanged(); }} />

      <Confirm
        open={!!removeTarget()}
        onClose={() => setRemoveTarget(null)}
        onConfirm={removeWord}
        danger
        loading={removeBusy()}
        title="移除词条"
        confirmText="移除"
        body={
          <Show when={removeTarget()}>
            {(t) => <>将从本词库移除「<b style={sx({ color: 'var(--text)' })}>{t().text}</b>」（单词本身不删除）。</>}
          </Show>
        }
      />
    </Card>
  );
}

/* ---------- remote catalog ---------- */

function RemoteCatalog(props: { onImported: () => void }) {
  const [configured, setConfigured] = createSignal(true);
  const [importing, setImporting] = createSignal<string | null>(null);
  const [syncing, setSyncing] = createSignal<string | null>(null);
  const [importTarget, setImportTarget] = createSignal<BrowseItem | null>(null);
  const [preview, setPreview] = createSignal<WordbookPreview | null>(null);
  const [uploadOpen, setUploadOpen] = createSignal(false);

  const [browse, { refetch: reloadBrowse }] = createResource(async (): Promise<BrowseItem[]> => {
    try {
      const settings = await adminApi.getSettings();
      if (!settings.wordbookCenterUrl) { setConfigured(false); return []; }
      setConfigured(true);
      return await adminApi.wbCenterBrowse();
    } catch (err: unknown) {
      setConfigured(false);
      toast.error('加载远程词库目录失败', err instanceof Error ? err.message : '');
      return [];
    }
  });

  const [updates, { refetch: reloadUpdates }] = createResource((): Promise<UpdateInfo[]> =>
    adminApi.wbCenterUpdates().catch(() => []));

  async function doImport(item: BrowseItem) {
    setImporting(item.id);
    try {
      const res = await adminApi.wbCenterImport(item.id);
      toast.success('已导入', `${res.wordbook.name} · ${res.wordsImported} 词`);
      await reloadBrowse();
      props.onImported();
    } catch (err: unknown) {
      toast.error(`导入「${item.name}」失败`, err instanceof Error ? err.message : '');
    } finally {
      setImporting(null);
    }
  }

  async function doSync(id: string, name: string) {
    setSyncing(id);
    try {
      const res = await adminApi.wbCenterSync(id);
      toast.success(`已同步 ${name}`, `新增 ${res.wordsAdded} · 更新 ${res.wordsUpdated} · 移除 ${res.wordsRemoved}`);
      await Promise.all([reloadBrowse(), reloadUpdates()]);
      props.onImported();
    } catch (err: unknown) {
      toast.error(`同步「${name}」失败`, err instanceof Error ? err.message : '');
    } finally {
      setSyncing(null);
    }
  }

  async function doPreview(id: string) {
    try {
      setPreview(await adminApi.wbCenterPreview(id, { perPage: 20 }));
    } catch (err: unknown) {
      toast.error('预览失败', err instanceof Error ? err.message : '');
    }
  }

  const busy = () => importing() !== null || syncing() !== null;

  return (
    <Card style={sx({ marginBottom: 16 })}>
      <div style={sx({ display: 'grid', gridTemplateColumns: '1.4fr 1fr', gap: 20 })} class="grid-collapse">
        {/* catalog */}
        <div>
          <div class="eyebrow" style={sx({ marginBottom: 10 })}>远程词库目录 · 词书中心</div>
          <Show
            when={configured()}
            fallback={<Empty title="尚未配置词书中心 URL" desc="请前往「系统设置」配置全局词书中心地址后再使用远程导入。" icon="remote" />}
          >
            <Show when={browse.state !== 'pending'} fallback={<Loading h={140} />}>
              <Show when={browse()!.length > 0} fallback={<Empty title="远程目录为空" desc="远程源中没有可用的词书。" icon="inbox" />}>
                <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fill,minmax(180px,1fr))', gap: 10 })}>
                  <For each={browse()}>
                    {(it) => (
                      <div style={sx({ padding: 12, borderRadius: 11, border: '1px solid var(--hairline)' })}>
                        <div style={sx({ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 6 })}>
                          <b style={sx({ fontSize: 13 })}>{it.name}</b>
                          <Show when={it.imported}>
                            <Badge variant={it.hasUpdate ? 'warning' : 'success'}>{it.hasUpdate ? '有更新' : '已导入'}</Badge>
                          </Show>
                        </div>
                        <div class="muted-3" style={sx({ fontSize: 11, margin: '4px 0 9px' })}>
                          {fmtNum(it.wordCount)} 词<Show when={it.version}> · v{it.version}</Show><Show when={it.author}> · {it.author}</Show>
                        </div>
                        <div style={sx({ display: 'flex', flexWrap: 'wrap', gap: 6 })}>
                          <Btn size="sm" variant="ghost" onClick={() => void doPreview(it.id)}>预览</Btn>
                          <Show when={!it.imported}>
                            <Btn size="sm" variant="primary" icon="download" disabled={busy()} onClick={() => setImportTarget(it)}>
                              {importing() === it.id ? '导入中…' : '导入'}
                            </Btn>
                          </Show>
                          <Show when={it.imported && it.hasUpdate}>
                            <Btn size="sm" variant="secondary" icon="refresh" disabled={busy()} onClick={() => void doSync(it.id, it.name)}>
                              {syncing() === it.id ? '同步中…' : '同步'}
                            </Btn>
                          </Show>
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </Show>
          </Show>
        </div>

        {/* updates + upload */}
        <div>
          <div class="eyebrow" style={sx({ marginBottom: 10 })}>可同步更新</div>
          <Show when={updates.state !== 'pending'} fallback={<Loading h={120} />}>
            <Show
              when={(updates()?.length ?? 0) > 0}
              fallback={<Empty title="均为最新" desc="所有已导入词书均为最新版本。" icon="check" />}
            >
              <For each={updates()}>
                {(u) => (
                  <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '10px 0', borderBottom: '1px solid var(--hairline)' })}>
                    <div style={sx({ minWidth: 0 })}>
                      <b style={sx({ fontSize: 13 })}>{u.name}</b>
                      <div class="muted-3" style={sx({ fontSize: 11 })}>v{u.localVersion} → v{u.remoteVersion}</div>
                    </div>
                    <Btn size="sm" variant="secondary" icon="refresh" disabled={busy()} onClick={() => void doSync(u.remoteId, u.name)}>
                      {syncing() === u.remoteId ? '同步中…' : '同步'}
                    </Btn>
                  </div>
                )}
              </For>
            </Show>
          </Show>
          <div style={sx({ marginTop: 14 })}>
            <Btn variant="outline" icon="upload" onClick={() => setUploadOpen(true)}>上传本地词库</Btn>
          </div>
        </div>
      </div>

      {/* import confirm */}
      <Confirm
        open={!!importTarget()}
        onClose={() => setImportTarget(null)}
        onConfirm={() => { const it = importTarget(); setImportTarget(null); if (it) void doImport(it); }}
        title="确认导入词库"
        confirmText="导入"
        body={
          <Show when={importTarget()}>
            {(it) => (
              <>
                将把「<b style={sx({ color: 'var(--text)' })}>{it().name}</b>」导入为系统词书（共 <span class="mono">{fmtNum(it().wordCount)}</span> 词）。
                <br />导入后系统词库将新增 / 合并对应单词。
              </>
            )}
          </Show>
        }
      />

      {/* preview modal */}
      <Modal open={preview() !== null} onClose={() => setPreview(null)} title={preview()?.name ?? '词书预览'} size="lg">
        <Show when={preview()}>
          {(p) => (
            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14, fontSize: 13 })}>
              <Show when={p().description}><p class="muted" style={sx({ lineHeight: 1.6 })}>{p().description}</p></Show>
              <div style={sx({ display: 'flex', gap: 16 })} class="muted">
                <span>词条 <span class="mono" style={sx({ color: 'var(--text)' })}>{fmtNum(p().wordCount)}</span></span>
                <Show when={p().version}><span>版本 <span class="mono" style={sx({ color: 'var(--text)' })}>v{p().version}</span></span></Show>
                <Show when={p().author}><span>作者 <span style={sx({ color: 'var(--text)' })}>{p().author}</span></span></Show>
              </div>
              <div>
                <div class="eyebrow" style={sx({ marginBottom: 8 })}>词条预览 · 前 {p().words.data.length} / {fmtNum(p().words.total)} 词</div>
                <div style={sx({ display: 'flex', flexDirection: 'column', gap: 8 })}>
                  <For each={p().words.data}>
                    {(w) => (
                      <div style={sx({ padding: '8px 12px', borderRadius: 9, background: 'var(--surface-sunken)' })}>
                        <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
                          <span style={sx({ fontWeight: 600 })}>{w.spelling}</span>
                          <Show when={w.phonetic}><span class="mono muted-3" style={sx({ fontSize: 11.5 })}>{w.phonetic}</span></Show>
                        </div>
                        <Show when={w.meanings.length > 0}>
                          <p class="muted" style={sx({ marginTop: 4, fontSize: 12.5, lineHeight: 1.5 })}>{w.meanings.join('；')}</p>
                        </Show>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </div>
          )}
        </Show>
      </Modal>

      <UploadWordbookModal open={uploadOpen()} onClose={() => setUploadOpen(false)} onDone={() => { void reloadBrowse(); props.onImported(); }} />
    </Card>
  );
}

/* ---------- modals ---------- */

function CreateWordbookModal(props: { open: boolean; onClose: () => void; onDone: (wb?: WordbookSummary) => void }) {
  const [name, setName] = createSignal('');
  const [desc, setDesc] = createSignal('');
  const [type, setType] = createSignal<WordbookType>('system');
  const [busy, setBusy] = createSignal(false);

  createEffect(() => { if (props.open) { setName(''); setDesc(''); setType('system'); } });

  async function submit() {
    if (!name().trim()) { toast.warning('请填写名称'); return; }
    setBusy(true);
    try {
      const wb = await adminApi.adminWordbookCreate({ name: name().trim(), description: desc().trim() || undefined, type: type() });
      toast.success('已创建词库', wb.name);
      props.onClose();
      props.onDone();
    } catch (err: unknown) {
      toast.error('创建失败', err instanceof Error ? err.message : '');
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={props.open} onClose={props.onClose} title="新建词库" size="sm"
      footer={<><Btn variant="ghost" onClick={props.onClose}>取消</Btn><Btn variant="primary" disabled={busy()} onClick={submit}>{busy() ? '创建中…' : '创建'}</Btn></>}
    >
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 13 })}>
        <Field label="词库名称"><input class="input" placeholder="如 CET-6 高频" value={name()} onInput={(e) => setName(e.currentTarget.value)} /></Field>
        <Field label="描述"><textarea class="textarea" rows={3} value={desc()} onInput={(e) => setDesc(e.currentTarget.value)} /></Field>
        <Field label="类型">
          <div style={sx({ display: 'flex', gap: 8 })}>
            <Btn variant={type() === 'system' ? 'primary' : 'secondary'} size="sm" onClick={() => setType('system')}>官方词库</Btn>
            <Btn variant={type() === 'user' ? 'primary' : 'secondary'} size="sm" onClick={() => setType('user')}>用户词库</Btn>
          </div>
        </Field>
      </div>
    </Modal>
  );
}

function EditWordbookModal(props: { open: boolean; wb: WordbookSummary | null; onClose: () => void; onDone: () => void }) {
  const [name, setName] = createSignal('');
  const [desc, setDesc] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  createEffect(() => {
    if (props.open && props.wb) { setName(props.wb.name); setDesc(props.wb.description); }
  });

  async function submit() {
    const wb = props.wb;
    if (!wb) return;
    if (!name().trim()) { toast.warning('请填写名称'); return; }
    setBusy(true);
    try {
      await adminApi.adminWordbookUpdate(wb.id, { name: name().trim(), description: desc().trim() });
      toast.success('已保存');
      props.onClose();
      props.onDone();
    } catch (err: unknown) {
      toast.error('保存失败', err instanceof Error ? err.message : '');
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={props.open} onClose={props.onClose} title="编辑词库" size="sm"
      footer={<><Btn variant="ghost" onClick={props.onClose}>取消</Btn><Btn variant="primary" disabled={busy()} onClick={submit}>{busy() ? '保存中…' : '保存'}</Btn></>}
    >
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 13 })}>
        <Field label="词库名称"><input class="input" value={name()} onInput={(e) => setName(e.currentTarget.value)} /></Field>
        <Field label="描述"><textarea class="textarea" rows={3} value={desc()} onInput={(e) => setDesc(e.currentTarget.value)} /></Field>
      </div>
    </Modal>
  );
}

function AddWordModal(props: { open: boolean; onClose: () => void; onDone: () => void; wbId: string }) {
  const [text, setText] = createSignal('');
  const [pron, setPron] = createSignal('');
  const [pos, setPos] = createSignal('');
  const [meaning, setMeaning] = createSignal('');
  const [examples, setExamples] = createSignal('');
  const [busy, setBusy] = createSignal(false);

  createEffect(() => { if (props.open) { setText(''); setPron(''); setPos(''); setMeaning(''); setExamples(''); } });

  async function submit() {
    if (!text().trim() || !meaning().trim()) { toast.warning('请填写单词与释义'); return; }
    setBusy(true);
    try {
      await adminApi.adminWordbookAddWord(props.wbId, {
        text: text().trim(),
        pronunciation: pron().trim() || undefined,
        partOfSpeech: pos().trim() || undefined,
        meaning: meaning().trim(),
        examples: examples().trim() ? examples().split('\n').map((s) => s.trim()).filter(Boolean) : undefined,
      });
      toast.success('已添加', text().trim());
      props.onClose();
      props.onDone();
    } catch (err: unknown) {
      toast.error('添加失败', err instanceof Error ? err.message : '');
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={props.open} onClose={props.onClose} title="添加词条" size="md"
      footer={<><Btn variant="ghost" onClick={props.onClose}>取消</Btn><Btn variant="primary" disabled={busy()} onClick={submit}>{busy() ? '添加中…' : '添加'}</Btn></>}
    >
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 13 })}>
        <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 })}>
          <Field label="单词"><input class="input" placeholder="如 tenacious" value={text()} onInput={(e) => setText(e.currentTarget.value)} /></Field>
          <Field label="音标(可选)"><input class="input" placeholder="/tɪˈneɪʃəs/" value={pron()} onInput={(e) => setPron(e.currentTarget.value)} /></Field>
        </div>
        <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 })}>
          <Field label="词性(可选)"><input class="input" placeholder="adj." value={pos()} onInput={(e) => setPos(e.currentTarget.value)} /></Field>
          <Field label="释义"><input class="input" placeholder="坚韧的 / 顽强的" value={meaning()} onInput={(e) => setMeaning(e.currentTarget.value)} /></Field>
        </div>
        <Field label="例句(可选,每行一条)">
          <textarea class="textarea" rows={3} placeholder="She was tenacious in her pursuit of justice." value={examples()} onInput={(e) => setExamples(e.currentTarget.value)} />
        </Field>
      </div>
    </Modal>
  );
}

function UploadWordbookModal(props: { open: boolean; onClose: () => void; onDone: () => void }) {
  const [id, setId] = createSignal('');
  const [name, setName] = createSignal('');
  const [desc, setDesc] = createSignal('');
  const [wordsJson, setWordsJson] = createSignal('[]');
  const [busy, setBusy] = createSignal(false);

  createEffect(() => { if (props.open) { setId(''); setName(''); setDesc(''); setWordsJson('[]'); } });

  function handleFile(file: File) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const txt = e.target?.result;
      if (typeof txt !== 'string') return;
      const baseId = file.name.replace(/\.(csv|json)$/i, '');
      if (file.name.toLowerCase().endsWith('.csv')) {
        const lines = txt.split(/\r?\n/).filter((l) => l.trim());
        const header = (lines[0] ?? '').split(',').map((h) => h.trim().toLowerCase());
        const idxOf = (...names: string[]) => header.findIndex((h) => names.includes(h));
        const byName = idxOf('word', 'spelling') >= 0;
        const sp = byName ? idxOf('word', 'spelling') : 0;
        const ph = byName ? idxOf('ipa', 'phonetic') : 1;
        const me = byName ? idxOf('gloss_zh', 'meaning', 'meanings') : 2;
        const ex = byName ? idxOf('example', 'examples') : -1;
        const words = lines.slice(1).map((line) => {
          const f = line.split(',').map((s) => s.trim());
          const meaning = me >= 0 ? (f[me] ?? '') : '';
          const example = ex >= 0 ? (f[ex] ?? '') : '';
          return {
            spelling: sp >= 0 ? (f[sp] ?? '') : '',
            phonetic: ph >= 0 && f[ph] ? f[ph] : undefined,
            meanings: meaning ? [meaning] : [],
            examples: example ? [example] : undefined,
          };
        }).filter((w) => w.spelling);
        setId((v) => v || baseId);
        setName((v) => v || baseId);
        setWordsJson(JSON.stringify(words, null, 2));
      } else {
        setId((v) => v || baseId);
        setName((v) => v || baseId);
        setWordsJson(txt);
      }
    };
    reader.readAsText(file);
  }

  function downloadTemplate() {
    const blob = new Blob([CSV_TEMPLATE], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'wordbook-template.csv';
    a.click();
    URL.revokeObjectURL(url);
  }

  async function submit() {
    if (!id().trim() || !name().trim()) { toast.warning('ID 与名称必填'); return; }
    let words: Array<{ spelling: string; phonetic?: string; meanings?: string[]; examples?: string[] }>;
    try {
      const parsed = JSON.parse(wordsJson());
      if (!Array.isArray(parsed)) throw new Error('words 应为数组');
      words = parsed;
    } catch (e) {
      toast.error('JSON 解析失败', e instanceof Error ? e.message : '');
      return;
    }
    setBusy(true);
    try {
      const res = await adminApi.wbCenterUpload({ id: id().trim(), name: name().trim(), description: desc().trim() || undefined, words });
      toast.success('已上传', `${res.wordbook.name} · ${res.wordsImported} 词`);
      props.onClose();
      props.onDone();
    } catch (err: unknown) {
      toast.error('上传失败', err instanceof Error ? err.message : '');
    } finally {
      setBusy(false);
    }
  }

  let fileInput: HTMLInputElement | undefined;

  return (
    <Modal
      open={props.open} onClose={props.onClose} title="上传本地词库" size="lg"
      footer={<><Btn variant="ghost" onClick={props.onClose}>取消</Btn><Btn variant="primary" disabled={busy()} onClick={submit}>{busy() ? '上传中…' : '上传'}</Btn></>}
    >
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 13 })}>
        <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 })}>
          <Field label="词书 ID"><input class="input" placeholder="如 cet4-core" value={id()} onInput={(e) => setId(e.currentTarget.value)} /></Field>
          <Field label="词书名称"><input class="input" placeholder="如 CET-4 核心词汇" value={name()} onInput={(e) => setName(e.currentTarget.value)} /></Field>
        </div>
        <Field label="简介(可选)"><input class="input" placeholder="一句话说明用途" value={desc()} onInput={(e) => setDesc(e.currentTarget.value)} /></Field>
        <Field label="Words JSON 数组" hint="CSV 格式：word,pos,ipa,gloss_zh,example(首行表头),按列名解析、客户端自动转 JSON">
          <div style={sx({ display: 'flex', gap: 8, marginBottom: 6 })}>
            <Btn size="sm" variant="secondary" icon="download" onClick={downloadTemplate}>下载模板</Btn>
            <Btn size="sm" variant="ghost" icon="upload" onClick={() => fileInput?.click()}>选择 .json/.csv 文件</Btn>
            <input
              ref={fileInput}
              type="file"
              accept=".json,.csv,application/json,text/csv"
              style={sx({ display: 'none' })}
              onChange={(e) => { const f = e.currentTarget.files?.[0]; if (f) handleFile(f); e.currentTarget.value = ''; }}
            />
          </div>
          <textarea
            class="textarea mono"
            rows={9}
            style={sx({ fontSize: 12 })}
            placeholder={'[{"spelling": "apple", "phonetic": "/ˈæpəl/", "meanings": ["苹果"]}]'}
            value={wordsJson()}
            onInput={(e) => setWordsJson(e.currentTarget.value)}
          />
        </Field>
      </div>
    </Modal>
  );
}
