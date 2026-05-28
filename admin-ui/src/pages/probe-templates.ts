/**
 * 远程探针 script 模板：admin 一键填入即可使用。
 * 所有模板假设 ctx_version=1（M2 schema：nav/perf/time/storage/idb/app/logs/errors/net）。
 */

export interface ProbeTemplate {
  name: string;
  description: string;
  body: string;
}

export const PROBE_TEMPLATES: ProbeTemplate[] = [
  {
    name: '设备指纹',
    description: 'UA / 语言 / 平台 / 硬件并发 / 设备内存 / 在线状态',
    body: `return {
  ua: ctx.nav.ua,
  language: ctx.nav.language,
  languages: ctx.nav.languages,
  platform: ctx.nav.platform,
  hardwareConcurrency: ctx.nav.hardwareConcurrency,
  deviceMemory: ctx.nav.deviceMemory,
  online: ctx.nav.online,
  connection: ctx.nav.connection,
  tz: ctx.time.tz,
  route: ctx.app.route,
  version: ctx.app.version,
};`,
  },
  {
    name: '内存 + 慢请求 Top 5',
    description: 'JS heap 占用 + 最近资源加载耗时排序',
    body: `return {
  memoryMB: ctx.perf.memoryMB(),
  summary: ctx.perf.resourceTimingSummary(),
};`,
  },
  {
    name: '最近 20 条错误',
    description: 'window.error + unhandledrejection 环形缓冲',
    body: `return {
  errors: ctx.errors.recent(20),
  errorCount: ctx.errors.recent(50).length,
};`,
  },
  {
    name: 'LocalStorage 摘要',
    description: 'key 列表 + 总字节（kv value 已脱敏）',
    body: `return {
  local: { keys: ctx.storage.keys('local'), size: ctx.storage.size('local') },
  session: { keys: ctx.storage.keys('session'), size: ctx.storage.size('session') },
};`,
  },
  {
    name: 'IndexedDB 与最近 10 网络',
    description: 'IDB 库列表 + 最近 fetch 调用',
    body: `return {
  idbs: ctx.idb.list(),
  recentNet: ctx.net.recent(10),
};`,
  },
];
