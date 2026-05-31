/* 各设置板块的专属渲染器(镜像设计稿 settings.html）。
 *
 * 与通用 renderFields 不同,这些渲染器把后端强类型 section（site/auth/ratelimit/
 * audit-config/backup-policy）的结构化字段映射成设计稿中的专属控件:
 * 品牌预览 / 色板取色器 / 外观模式 seg / 第三方登录 provider 卡片网格 / 各类 seg 单选 /
 * 2FA 三档开关。
 *
 * 编辑模型:渲染器拿到该 section 的「合并后实时值」(value)与 onPatch。任何改动都把
 * 「完整 section 对象」回传 onPatch —— 因为后端 typed section 做 strict serde 解析,
 * 缺字段会 422,所以 patch 必须是完整体。
 *
 * 诚实降级(不杜撰数据):
 *   - JWT secret / 轮换:后端 JWT 密钥来自启动期 config(env),settings 存储里没有该
 *     字段、也无运行期轮换端点,故不渲染假的 secret 输入框 / 轮换按钮。
 *   - 备份「最近备份」历史:后端无该数据源,不渲染杜撰的历史表格。
 */
import { For, Show } from 'solid-js';
import { Row, Field, Seg } from './SectionCard';

type Obj = Record<string, unknown>;

export interface SectionEditor {
  /** 该 section 的合并后实时值(baseline + patch)。 */
  value: () => Obj;
  /** 写回完整 section 对象。 */
  onPatch: (next: Obj) => void;
}

/* ── 小工具 ── */
function clone(v: Obj): Obj {
  return JSON.parse(JSON.stringify(v ?? {}));
}
function asStr(v: unknown, d = ''): string {
  return v == null ? d : String(v);
}
function asNum(v: unknown, d = 0): number {
  const n = Number(v);
  return Number.isFinite(n) ? n : d;
}
function asObj(v: unknown): Obj {
  return v && typeof v === 'object' && !Array.isArray(v) ? (v as Obj) : {};
}
const MASK = '••••••';
const isMasked = (v: unknown) => typeof v === 'string' && /^[•*]{3,}$/.test(v.trim());

/** 计算前景文本(白/黑)对该背景色的近似 WCAG 对比度。背景非 hex 时返回 null。 */
function contrastForHex(hex: string): { ratio: number; passAA: boolean } | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const int = parseInt(m[1], 16);
  const srgb = [(int >> 16) & 255, (int >> 8) & 255, int & 255].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  const lum = 0.2126 * srgb[0] + 0.7152 * srgb[1] + 0.0722 * srgb[2];
  // 对白色文本的对比度
  const ratioWhite = 1.05 / (lum + 0.05);
  const ratioBlack = (lum + 0.05) / 0.05;
  const ratio = Math.max(ratioWhite, ratioBlack);
  return { ratio, passAA: ratio >= 4.5 };
}

/* ───────────────────────────── SITE ───────────────────────────── */
export function SiteRenderer(props: SectionEditor) {
  const v = () => props.value();
  const patch = (k: string, val: unknown) => {
    const next = clone(v());
    next[k] = val;
    props.onPatch(next);
  };
  const accent = () => asStr(v().accent);
  const accentHex = () => {
    const m = /^#?([0-9a-f]{6})$/i.exec(accent().trim());
    return m ? `#${m[1]}` : '#5b6dff';
  };
  const contrast = () => contrastForHex(accent());

  return (
    <>
      <Row label="品牌名称 / Logo" hint="在登录页、邮件、PWA manifest、404 页面统一显示。修改后约 60 秒生效。">
        <div class="brand-prev">
          <div class="logo-block">
            <Show when={asStr(v().logoUrl)} fallback={asStr(v().brand, 'W').slice(0, 1).toUpperCase()}>
              <img src={asStr(v().logoUrl)} alt="logo" />
            </Show>
          </div>
          <div class="meta">
            <strong>{asStr(v().brand, 'WordForge')}</strong>
            <span class="tag">{asStr(v().subtitle, '—')}</span>
            <div class="swatches">
              <span style={{ background: accent() || 'var(--accent)' }} />
              <span style={{ background: 'oklch(46% 0.22 290)' }} />
              <span style={{ background: 'var(--success)' }} />
              <span style={{ background: 'var(--content)' }} />
              <span style={{ background: 'var(--surface)' }} />
            </div>
          </div>
        </div>
        <div class="gap-2 wrap" style={{ 'margin-top': '10px' }}>
          <Field value={v().brand} size="wide" width="220px" onChange={(val) => patch('brand', val)} placeholder="品牌名称" />
          <Field value={v().subtitle} size="wide" width="320px" onChange={(val) => patch('subtitle', val)} placeholder="副标题" />
        </div>
        <div class="gap-2" style={{ 'margin-top': '6px' }}>
          <span class="helper" style={{ width: '60px' }}>logo</span>
          <Field value={v().logoUrl} mono size="wide" width="420px" placeholder="https://…(可选)" onChange={(val) => patch('logoUrl', val)} />
        </div>
      </Row>

      <Row label="站点 URL" hint="用于 OAuth 回调、邮件链接、CDN 校验。">
        <div class="field-group">
          <div class="row"><span class="helper" style={{ width: '70px' }}>canonical</span><Field value={v().canonicalUrl} mono size="wide" onChange={(val) => patch('canonicalUrl', val)} /></div>
          <div class="row"><span class="helper" style={{ width: '70px' }}>admin</span><Field value={v().adminUrl} mono size="wide" onChange={(val) => patch('adminUrl', val)} /></div>
          <div class="row"><span class="helper" style={{ width: '70px' }}>cdn</span><Field value={v().cdnUrl} mono size="wide" onChange={(val) => patch('cdnUrl', val)} /></div>
        </div>
      </Row>

      <Row label="默认时区 / 语言" hint="新用户首次注册时的默认值,后续以用户偏好为准。">
        <div class="gap-2 wrap">
          <Field value={v().timezone} size="medium" placeholder="Asia/Shanghai" onChange={(val) => patch('timezone', val)} />
          <Field value={v().language} size="medium" placeholder="zh-CN" onChange={(val) => patch('language', val)} />
        </div>
      </Row>

      <Row label="外观模式" hint="影响管理后台与官网默认主题。用户可独立覆盖。">
        <Seg
          value={asStr(v().theme, 'system')}
          options={[{ value: 'light', label: 'Light' }, { value: 'dark', label: 'Dark' }, { value: 'system', label: '跟随系统' }]}
          onChange={(val) => patch('theme', val)}
        />
      </Row>

      <Row label="主色调" hint="影响前后台所有强调色 · 立即热加载,无需刷新">
        <div class="gap-2" style={{ 'align-items': 'center' }}>
          <input
            type="color"
            class="st-color"
            value={accentHex()}
            onInput={(e) => patch('accent', e.currentTarget.value)}
          />
          <Field value={v().accent} mono size="wide" width="200px" placeholder="oklch(…) / #hex" onChange={(val) => patch('accent', val)} />
          <Show when={contrast()}>
            <span class={`conn-pill ${contrast()!.passAA ? 'ok' : 'warn'}`}>
              <span class="dot" /> 对比度 {contrast()!.passAA ? 'AA' : '不足'} · {contrast()!.ratio.toFixed(1)}:1
            </span>
          </Show>
        </div>
      </Row>

      <Row label="SEO meta" hint="用于 og:image、twitter:card、搜索引擎索引。">
        <textarea
          class="st-input wide"
          style={{ height: '74px' }}
          value={asStr(v().seo)}
          onInput={(e) => patch('seo', e.currentTarget.value)}
        />
      </Row>
    </>
  );
}

/* ───────────────────────────── AUTH ───────────────────────────── */
const PROVIDERS: { key: string; cls: string; pic: string; name: string }[] = [
  { key: 'google', cls: 'is-google', pic: 'G', name: 'Google' },
  { key: 'apple', cls: 'is-apple', pic: '', name: 'Apple' },
  { key: 'wechat', cls: 'is-wechat', pic: 'W', name: '微信' },
  { key: 'qq', cls: 'is-qq', pic: 'QQ', name: 'QQ' },
  { key: 'github', cls: 'is-github', pic: 'G', name: 'GitHub' },
  { key: 'magicLink', cls: 'is-mail', pic: '@', name: '魔法链接' },
];

export function AuthRenderer(props: SectionEditor) {
  const v = () => props.value();
  const jwt = () => asObj(v().jwt);
  const oauth = () => asObj(v().oauth);
  const reg = () => asObj(v().registration);
  const tf = () => asObj(v().twoFactor);
  const lk = () => asObj(v().lockout);
  const pw = () => asObj(v().password);

  const patchNested = (group: string, k: string, val: unknown) => {
    const next = clone(v());
    next[group] = { ...asObj(next[group]), [k]: val };
    props.onPatch(next);
  };
  const toggleProvider = (k: string) => {
    const next = clone(v());
    const cur = asObj(next.oauth);
    next.oauth = { ...cur, [k]: !cur[k] };
    props.onPatch(next);
  };

  return (
    <>
      <Row label="JWT 配置" hint="影响所有客户端 token 有效期、刷新策略与算法。" metaPill="SUPER-ADMIN ONLY">
        <div class="field-group">
          <div class="row"><span class="helper" style={{ width: '90px' }}>access_ttl</span><Field value={jwt().accessTtlMinutes} type="number" size="compact" onChange={(val) => patchNested('jwt', 'accessTtlMinutes', val === '' ? '' : Number(val))} /><span class="helper">分钟</span></div>
          <div class="row"><span class="helper" style={{ width: '90px' }}>refresh_ttl</span><Field value={jwt().refreshTtlDays} type="number" size="compact" onChange={(val) => patchNested('jwt', 'refreshTtlDays', val === '' ? '' : Number(val))} /><span class="helper">天</span></div>
          <div class="row"><span class="helper" style={{ width: '90px' }}>rotation</span><label class="st-switch"><input type="checkbox" checked={!!jwt().rotation} onChange={(e) => patchNested('jwt', 'rotation', e.currentTarget.checked)} /><span class="slider" /></label><span class="helper">刷新轮换</span></div>
          <div class="row"><span class="helper" style={{ width: '90px' }}>algorithm</span>
            <select class="st-input compact" style={{ width: '130px' }} value={asStr(jwt().algorithm, 'RS256')} onChange={(e) => patchNested('jwt', 'algorithm', e.currentTarget.value)}>
              <option value="HS256">HS256</option><option value="RS256">RS256</option><option value="ES256">ES256</option>
            </select>
          </div>
        </div>
        <p class="helper" style={{ 'margin-top': '8px' }}>JWT 签名密钥由部署期环境变量管理,不在此面板存储或轮换。</p>
      </Row>

      <Row label="第三方登录" hint="启用的渠道会出现在登录页。开关即生效;client_id / secret 由部署期配置注入。">
        <div class="prov-grid">
          <For each={PROVIDERS}>
            {(p) => {
              const on = () => !!oauth()[p.key];
              return (
                <div class={`prov-card ${p.cls} ${on() ? 'is-on' : ''}`} role="button" tabindex="0" onClick={() => toggleProvider(p.key)}>
                  <div class="pic">{p.pic}</div>
                  <div><div class="name">{p.name}</div><div class="meta">{on() ? '已启用' : '未启用'}</div></div>
                  <div class="check" />
                </div>
              );
            }}
          </For>
        </div>
      </Row>

      <Row label="注册策略" hint="控制谁可以注册账号。封闭社区可以禁用注册仅靠邀请。">
        <Seg
          value={asStr(reg().policy, 'open')}
          options={[
            { value: 'open', label: '开放注册' },
            { value: 'invite_only', label: '仅邀请码' },
            { value: 'enterprise_domain', label: '仅企业域名' },
            { value: 'closed', label: '关闭' },
          ]}
          onChange={(val) => patchNested('registration', 'policy', val)}
        />
        <Show when={asStr(reg().policy) === 'enterprise_domain'}>
          <div class="gap-2" style={{ 'margin-top': '8px' }}>
            <span class="helper">允许域名</span>
            <Field
              value={(Array.isArray(reg().allowedDomains) ? (reg().allowedDomains as string[]) : []).join(', ')}
              mono size="wide" width="280px" placeholder="@acme.com, @sub.acme.com"
              onChange={(val) => patchNested('registration', 'allowedDomains', val.split(',').map((s) => s.trim()).filter(Boolean))}
            />
          </div>
        </Show>
      </Row>

      <Row label="双因素 (2FA)" hint="建议管理员强制开启 TOTP,普通用户可选。">
        <div class="field-group">
          <For each={[{ k: 'admin', label: '管理员' }, { k: 'paid', label: '付费用户' }, { k: 'regular', label: '普通用户' }]}>
            {(r) => (
              <div class="row" style={{ gap: '14px' }}>
                <span style={{ font: '500 12.5px/1 var(--font-display)', width: '120px' }}>{r.label}</span>
                <Seg
                  value={asStr(tf()[r.k], 'optional')}
                  options={[{ value: 'off', label: '禁用' }, { value: 'optional', label: '可选' }, { value: 'required', label: '强制' }]}
                  onChange={(val) => patchNested('twoFactor', r.k, val)}
                />
              </div>
            )}
          </For>
        </div>
      </Row>

      <Row label="登录锁定" hint="异常尝试触发后等待时长,使用滑动窗口。">
        <div class="gap-2 wrap">
          <Field value={lk().maxFailures} type="number" size="compact" onChange={(val) => patchNested('lockout', 'maxFailures', val === '' ? '' : Number(val))} />
          <span class="helper">次失败 /</span>
          <Field value={lk().windowMinutes} type="number" size="compact" onChange={(val) => patchNested('lockout', 'windowMinutes', val === '' ? '' : Number(val))} />
          <span class="helper">分钟内 → 锁定</span>
          <Field value={lk().lockMinutes} type="number" size="compact" onChange={(val) => patchNested('lockout', 'lockMinutes', val === '' ? '' : Number(val))} />
          <span class="helper">分钟</span>
        </div>
      </Row>

      <Row label="密码策略" hint="由 zxcvbn 评分,弱密码会被拒绝。">
        <div class="field-group">
          <div class="row" style={{ gap: '14px' }}><span class="helper" style={{ width: '120px' }}>最低长度</span><Field value={pw().minLength} type="number" size="compact" onChange={(val) => patchNested('password', 'minLength', val === '' ? '' : Number(val))} /></div>
          <div class="row" style={{ gap: '14px' }}><span class="helper" style={{ width: '120px' }}>最小 zxcvbn 评分</span>
            <select class="st-input compact" style={{ width: '80px' }} value={asNum(pw().minZxcvbnScore, 3)} onChange={(e) => patchNested('password', 'minZxcvbnScore', Number(e.currentTarget.value))}>
              <option value="0">0</option><option value="1">1</option><option value="2">2</option><option value="3">3</option><option value="4">4</option>
            </select>
          </div>
          <div class="row" style={{ gap: '14px' }}><span class="helper" style={{ width: '120px' }}>禁用历史</span><Field value={pw().historyCount} type="number" size="compact" onChange={(val) => patchNested('password', 'historyCount', val === '' ? '' : Number(val))} /><span class="helper">个最近密码</span></div>
        </div>
      </Row>
    </>
  );
}

/* ───────────────────────────── RATE LIMIT ───────────────────────────── */
export function RatelimitRenderer(props: SectionEditor) {
  const v = () => props.value();
  const ipb = () => asObj(v().ipBlocklist);
  const sensitive = () => (Array.isArray(v().sensitive) ? (v().sensitive as Obj[]) : []);
  const patch = (k: string, val: unknown) => { const next = clone(v()); next[k] = val; props.onPatch(next); };
  const patchIpb = (k: string, val: unknown) => { const next = clone(v()); next.ipBlocklist = { ...asObj(next.ipBlocklist), [k]: val }; props.onPatch(next); };
  const patchSensitive = (i: number, k: string, val: unknown) => {
    const next = clone(v());
    const arr = (Array.isArray(next.sensitive) ? next.sensitive : []) as Obj[];
    arr[i] = { ...asObj(arr[i]), [k]: val };
    next.sensitive = arr;
    props.onPatch(next);
  };
  return (
    <>
      <Row label="全局 RPS 上限" hint="所有 API 入口的硬上限,超过后返回 429 + Retry-After。">
        <div class="gap-2"><Field value={v().globalRps} type="number" size="compact" width="120px" onChange={(val) => patch('globalRps', val === '' ? '' : Number(val))} /><span class="helper">RPS</span></div>
      </Row>
      <Row label="单用户每分钟" hint="区分匿名 / 已认证 / 付费 · 防爬虫与刷流。">
        <div class="field-group">
          <div class="row"><span class="helper" style={{ width: '80px' }}>匿名</span><Field value={v().anonRpm} type="number" size="compact" onChange={(val) => patch('anonRpm', val === '' ? '' : Number(val))} /><span class="helper">RPM</span></div>
          <div class="row"><span class="helper" style={{ width: '80px' }}>已认证</span><Field value={v().authRpm} type="number" size="compact" onChange={(val) => patch('authRpm', val === '' ? '' : Number(val))} /><span class="helper">RPM</span></div>
          <div class="row"><span class="helper" style={{ width: '80px' }}>付费</span><Field value={v().paidRpm} type="number" size="compact" onChange={(val) => patch('paidRpm', val === '' ? '' : Number(val))} /><span class="helper">RPM</span></div>
        </div>
      </Row>
      <Show when={sensitive().length > 0}>
        <Row label="敏感接口加严" hint="单独配额(次数 / 窗口秒 / 维度)。">
          <div class="field-group">
            <For each={sensitive()}>
              {(s, i) => (
                <div class="row">
                  <Field value={s.path} mono size="wide" width="200px" placeholder="POST /auth/login" onChange={(val) => patchSensitive(i(), 'path', val)} />
                  <Field value={s.limit} type="number" size="compact" onChange={(val) => patchSensitive(i(), 'limit', val === '' ? '' : Number(val))} />
                  <span class="helper">次 /</span>
                  <Field value={s.windowSecs} type="number" size="compact" onChange={(val) => patchSensitive(i(), 'windowSecs', val === '' ? '' : Number(val))} />
                  <span class="helper">秒 /</span>
                  <Seg value={asStr(s.scope, 'ip')} options={[{ value: 'ip', label: 'IP' }, { value: 'uid', label: 'uid' }]} onChange={(val) => patchSensitive(i(), 'scope', val)} />
                </div>
              )}
            </For>
          </div>
        </Row>
      </Show>
      <Row label="IP 拉黑" hint="触发后自动加入 deny list。">
        <div class="gap-2 wrap">
          <Field value={ipb().thresholdPerHour} type="number" size="compact" onChange={(val) => patchIpb('thresholdPerHour', val === '' ? '' : Number(val))} />
          <span class="helper">次 429 / 小时 → 自动拉黑</span>
          <Field value={ipb().banHours} type="number" size="compact" onChange={(val) => patchIpb('banHours', val === '' ? '' : Number(val))} />
          <span class="helper">小时</span>
        </div>
      </Row>
    </>
  );
}

/* ───────────────────────────── AUDIT ───────────────────────────── */
export function AuditRenderer(props: SectionEditor) {
  const v = () => props.value();
  const siem = () => asObj(v().siem);
  const patch = (k: string, val: unknown) => { const next = clone(v()); next[k] = val; props.onPatch(next); };
  const patchSiem = (k: string, val: unknown) => { const next = clone(v()); next.siem = { ...asObj(next.siem), [k]: val }; props.onPatch(next); };
  return (
    <>
      <Row label="记录范围" hint="建议至少 90 天,合规要求一般 1 年以上。">
        <div class="gap-2"><Field value={v().retentionDays} type="number" size="compact" onChange={(val) => patch('retentionDays', val === '' ? '' : Number(val))} /><span class="helper">天</span></div>
      </Row>
      <Row label="不可篡改" hint="每条记录 SHA256 链,写入后立刻发布 anchor。">
        <label class="st-switch"><input type="checkbox" checked={!!v().hashChain} onChange={(e) => patch('hashChain', e.currentTarget.checked)} /><span class="slider" /></label>
        <span class="helper" style={{ 'margin-left': '10px', display: 'inline-block' }}>{v().hashChain ? '已启用哈希链' : '未启用'}</span>
      </Row>
      <Row label="SIEM 转发" hint="同步到 Splunk / ELK / Datadog Logs。">
        <Seg
          value={asStr(siem().target, 'none')}
          options={[{ value: 'none', label: '关闭' }, { value: 'elk', label: 'ELK' }, { value: 'datadog', label: 'Datadog' }, { value: 'splunk', label: 'Splunk' }]}
          onChange={(val) => patchSiem('target', val)}
        />
        <Show when={asStr(siem().target) !== 'none'}>
          <div class="gap-2" style={{ 'margin-top': '8px' }}>
            <span class="helper">target</span>
            <Field value={siem().url} mono size="wide" width="320px" placeholder="https://intake.…" onChange={(val) => patchSiem('url', val)} />
          </div>
        </Show>
      </Row>
    </>
  );
}

/* ───────────────────────────── BACKUP ───────────────────────────── */
export function BackupRenderer(props: SectionEditor) {
  const v = () => props.value();
  const patch = (k: string, val: unknown) => { const next = clone(v()); next[k] = val; props.onPatch(next); };
  const targets = () => (Array.isArray(v().targets) ? (v().targets as Obj[]) : []);
  const patchTarget = (i: number, k: string, val: unknown) => {
    const next = clone(v());
    const arr = (Array.isArray(next.targets) ? next.targets : []) as Obj[];
    arr[i] = { ...asObj(arr[i]), [k]: val };
    next.targets = arr;
    props.onPatch(next);
  };
  return (
    <>
      <Row label="调度" hint="cron 表达式 · 失败自动重试 · 告警至 oncall。">
        <div class="field-group">
          <div class="row"><span class="helper" style={{ width: '90px' }}>full</span><Field value={v().fullCron} mono size="wide" width="160px" placeholder="0 3 * * 0" onChange={(val) => patch('fullCron', val)} /><span class="helper">全量</span></div>
          <div class="row"><span class="helper" style={{ width: '90px' }}>incremental</span><Field value={v().incrementalCron} mono size="wide" width="160px" placeholder="0 */6 * * *" onChange={(val) => patch('incrementalCron', val)} /><span class="helper">增量</span></div>
          <div class="row"><span class="helper" style={{ width: '90px' }}>WAL ship</span><label class="st-switch"><input type="checkbox" checked={!!v().walArchive} onChange={(e) => patch('walArchive', e.currentTarget.checked)} /><span class="slider" /></label><span class="helper">流式归档</span></div>
        </div>
      </Row>
      <Row label="目标 / 保留" hint="多区域备份 + 冷归档。">
        <div class="field-group">
          <For each={targets()}>
            {(t, i) => (
              <div class="row">
                <Field value={t.name} mono size="wide" width="90px" placeholder="primary" onChange={(val) => patchTarget(i(), 'name', val)} />
                <Field value={t.uri} mono size="wide" placeholder="s3://…" onChange={(val) => patchTarget(i(), 'uri', val)} />
                <Field value={t.retentionDays} type="number" size="compact" onChange={(val) => patchTarget(i(), 'retentionDays', val === '' ? '' : Number(val))} />
                <span class="helper">天</span>
              </div>
            )}
          </For>
          <Show when={targets().length === 0}>
            <p class="helper">暂无备份目标(后端未下发)。</p>
          </Show>
        </div>
      </Row>
    </>
  );
}
