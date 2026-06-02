import { createSignal, onMount, onCleanup, Show, createMemo, createResource } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { themeStore } from '@/stores/theme';
import { uiStore } from '@/stores/ui';
import { ADMIN_MAX_LOCK_WAIT_SECS } from '@/lib/constants';

/**
 * 管理员登录 —— 按 admin后端/login.html 双栏设计。
 *
 * 左栏：brand + 表单 + footer 状态行
 * 右栏（≤920px 隐藏）：AMAS 品牌叙事 + 公共健康卡（UPTIME/SLO/DB SIZE 来自 /health 公共端点，
 *   SLO 卡按 availability.effectiveSecs 动态标注真实测量窗口，无样本时降级 "—"）
 *
 * 现有锁定 / 失败计数 / token 恢复逻辑全部保留。
 */
export default function LoginPage() {
  const navigate = useNavigate();
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [showPw, setShowPw] = createSignal(false);
  const [rememberMe, setRememberMe] = createSignal(true); // 30 天保持登录,默认勾选(与设计图一致)
  const [loading, setLoading] = createSignal(false);
  const [emailError, setEmailError] = createSignal('');
  const [formError, setFormError] = createSignal('');
  const [failCount, setFailCount] = createSignal(0);
  const [lockUntil, setLockUntil] = createSignal(0);
  const [lockSeconds, setLockSeconds] = createSignal(0);
  const [clock, setClock] = createSignal(new Date().toLocaleTimeString('zh-CN', { hour12: false }));
  let lockTimer: ReturnType<typeof setInterval> | undefined;
  let clockTimer: ReturnType<typeof setInterval> | undefined;

  // 公共健康端点：未登录可访问，透出 version / uptimeSecs / availability / dbSizeBytes
  const [publicHealth] = createResource(() => adminApi.health().catch(() => null));
  // HealthInfo 仅强类型化 version/dbSizeBytes,其余字段经 [key:string]:unknown 落地;此处收敛为数值或 null
  const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);
  const uptimeSecs = () => num(publicHealth()?.uptimeSecs);
  // 可用性：来自 /health 的真实聚合(availability.pct)。窗口随进程运行时长增长(≤30d)，
  // effectiveSecs 透出实际测量窗口,据此动态标注卡片标题,不冒充满 30 天。
  const availability = () => publicHealth()?.availability ?? null;
  const sloPct = () => num(availability()?.pct);
  const sloEffectiveSecs = () => num(availability()?.effectiveSecs);
  // 标签：实际窗口 ≥30d 显示 "SLO 30d"，否则按真实天/时降级("SLO 7d"/"SLO 12h")。
  const sloLabel = () => {
    const s = sloEffectiveSecs();
    if (s == null) return 'SLO 30d';
    const d = Math.floor(s / 86400);
    if (d >= 30) return 'SLO 30d';
    if (d >= 1) return `SLO ${d}d`;
    const h = Math.max(1, Math.floor(s / 3600));
    return `SLO ${h}h`;
  };
  const dbSizeBytes = () => num(publicHealth()?.dbSizeBytes);
  const appVersion = () => { const v = publicHealth()?.version; return typeof v === 'string' && v ? v : null; };

  // 密码强度：3 级（≥8 / ≥8+大小写+数字 / ≥14+符号）
  const pwStrength = createMemo<0 | 1 | 2 | 3>(() => {
    const v = password();
    let s = 0;
    if (v.length >= 8) s++;
    if (/[A-Z]/.test(v) && /[a-z]/.test(v) && /\d/.test(v)) s++;
    if (v.length >= 14 && /[^A-Za-z0-9]/.test(v)) s++;
    return s as 0 | 1 | 2 | 3;
  });

  function formatUptime(secs: number | null | undefined): string {
    if (!secs || secs < 0) return '—';
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    return `${d}d ${h}h`;
  }

  // DB SIZE：字节 → MB/GB,无权限(null)优雅省略为 —
  function formatBytes(bytes: number | null | undefined): string {
    if (bytes == null || bytes < 0) return '—';
    if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
    return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
  }

  onMount(async () => {
    lockTimer = setInterval(() => {
      const remaining = Math.ceil((lockUntil() - Date.now()) / 1000);
      setLockSeconds(remaining > 0 ? remaining : 0);
    }, 1000);
    clockTimer = setInterval(() => {
      setClock(new Date().toLocaleTimeString('zh-CN', { hour12: false }));
    }, 1000);

    const existingToken = tokenManager.getAdminToken();
    if (existingToken) {
      try {
        await adminApi.getHealth();
        navigate('/admin', { replace: true });
        return;
      } catch {
        tokenManager.clearAdminToken();
      }
    }

    try {
      const status = await adminApi.checkStatus();
      if (!status.initialized) {
        navigate('/admin/setup', { replace: true });
      }
    } catch { /* ignore */ }
  });

  onCleanup(() => {
    if (lockTimer) clearInterval(lockTimer);
    if (clockTimer) clearInterval(clockTimer);
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    setEmailError('');
    setFormError('');
    if (!email()) { setEmailError('请填写邮箱'); return; }
    if (!password()) { setFormError('请填写密码'); return; }

    if (lockSeconds() > 0) {
      setFormError(`请等待 ${lockSeconds()} 秒后重试`);
      return;
    }

    setLoading(true);
    try {
      const res = await adminApi.login({ email: email(), password: password(), rememberMe: rememberMe() });
      setPassword('');
      setFailCount(0);
      setLockUntil(0);
      setLockSeconds(0);
      tokenManager.setAdminToken(res.token);
      uiStore.toast.success('管理员登录成功');
      navigate('/admin', { replace: true });
    } catch (err: unknown) {
      const count = failCount() + 1;
      setFailCount(count);
      const waitSeconds = Math.min(count, ADMIN_MAX_LOCK_WAIT_SECS);
      setLockUntil(Date.now() + waitSeconds * 1000);
      setLockSeconds(waitSeconds);
      const msg = err instanceof Error ? err.message : '登录失败';
      setFormError(count >= 3 ? `${msg}（已失败 ${count} 次，请等待 ${waitSeconds} 秒）` : msg);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div class="grid min-h-screen bg-surface-secondary overflow-hidden" style={{
      'grid-template-columns': 'minmax(0, 1fr) minmax(0, 1.05fr)',
      'padding-top': 'max(0px, env(safe-area-inset-top))',
      'padding-bottom': 'max(0px, env(safe-area-inset-bottom))',
    }}>
      {/* 左栏 —— 表单 */}
      <div class="flex flex-col px-6 py-6 sm:px-11 sm:py-8 bg-surface min-h-screen min-w-0 col-span-2 md:col-span-1">
        {/* topbar */}
        <div class="flex items-center justify-between mb-auto">
          <div class="flex items-center gap-2.5">
            <div
              class="w-[30px] h-[30px] rounded-lg grid place-items-center text-white font-bold text-sm tracking-tight"
              style={{ background: 'linear-gradient(140deg, var(--accent), oklch(46% 0.22 290))' }}
              aria-hidden="true"
            >W</div>
            <strong class="text-sm font-semibold tracking-tight text-content">WordForge Admin</strong>
          </div>
          <button
            type="button"
            class="size-9 rounded-md grid place-items-center text-content-secondary hover:bg-surface-secondary transition-colors focus-ring-soft"
            onClick={() => themeStore.toggle()}
            aria-label={`切换主题（当前 ${themeStore.mode()}）`}
            title={`主题：${themeStore.mode() === 'light' ? '亮色' : themeStore.mode() === 'dark' ? '暗色' : '跟随系统'}`}
          >
            <Show when={themeStore.effective() === 'dark'} fallback={
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" /></svg>
            }>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" /></svg>
            </Show>
          </button>
        </div>

        {/* center card */}
        <div class="w-full max-w-[420px] mx-auto my-10 animate-fade-in-up">
          <h1 class="text-display-sm font-bold tracking-tight text-content mb-1.5">登录管理后台</h1>
          <p class="text-content-secondary text-[13.5px] mb-6">使用初始化时生成的管理员邮箱与 ADMIN_JWT_SECRET 派发的凭证登录。</p>

          {/* form-top error toast（shake animation） */}
          <Show when={formError()}>
            <div
              role="alert"
              class="flex items-center gap-1.5 mb-3 rounded-md bg-error-light text-error-strong px-3 py-2 text-[12.5px] animate-shake"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                <circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /><line x1="12" y1="16" x2="12.01" y2="16" />
              </svg>
              {formError()}
            </div>
          </Show>

          <form onSubmit={handleSubmit} autocomplete="on" novalidate class="space-y-3.5">
            {/* email */}
            <div>
              <label for="login-email" class="flex justify-between items-baseline text-[12.5px] text-content-secondary mb-1.5">
                <span>管理员邮箱</span>
              </label>
              <div class="relative">
                <svg
                  class="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-content-tertiary"
                  viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"
                >
                  <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z" />
                  <polyline points="22,6 12,13 2,6" />
                </svg>
                <input
                  id="login-email"
                  type="email"
                  required
                  autocomplete="email"
                  disabled={loading() || lockSeconds() > 0}
                  value={email()}
                  onInput={(e) => setEmail(e.currentTarget.value)}
                  placeholder="admin@wordforge.local"
                  class="w-full h-10 pl-10 pr-3 rounded-md border border-border bg-surface text-[13.5px] text-content placeholder:text-content-tertiary focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/30 transition disabled:opacity-60"
                  aria-invalid={!!emailError()}
                />
              </div>
              <Show when={emailError()}>
                <p class="mt-1 text-[11.5px] text-error">{emailError()}</p>
              </Show>
            </div>

            {/* password */}
            <div>
              <div class="flex justify-between items-baseline mb-1.5">
                <label for="login-pw" class="text-[12.5px] text-content-secondary">密码</label>
                <span class="text-[11.5px] text-content-tertiary cursor-default" title="如忘记密码，请在服务器端 reset 或重新生成 ADMIN_JWT_SECRET">忘记密码？联系 ops</span>
              </div>
              <div class="relative">
                <svg
                  class="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-content-tertiary"
                  viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"
                >
                  <rect x="3" y="11" width="18" height="11" rx="2" />
                  <path d="M7 11V7a5 5 0 0 1 10 0v4" />
                </svg>
                <input
                  id="login-pw"
                  type={showPw() ? 'text' : 'password'}
                  required
                  autocomplete="current-password"
                  disabled={loading() || lockSeconds() > 0}
                  value={password()}
                  onInput={(e) => setPassword(e.currentTarget.value)}
                  placeholder="至少 12 位 · 含字母数字符号"
                  class="w-full h-10 pl-10 pr-10 rounded-md border border-border bg-surface text-[13.5px] text-content placeholder:text-content-tertiary focus:outline-none focus:border-accent focus:ring-2 focus:ring-accent/30 transition disabled:opacity-60"
                />
                <button
                  type="button"
                  class="absolute right-2 top-1/2 -translate-y-1/2 size-7 rounded grid place-items-center text-content-tertiary hover:text-content focus-ring-soft"
                  onClick={() => setShowPw(!showPw())}
                  aria-label={showPw() ? '隐藏密码' : '显示密码'}
                  tabindex={-1}
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" /><circle cx="12" cy="12" r="3" />
                  </svg>
                </button>
              </div>
              {/* 强度计 */}
              <div class="flex gap-1 mt-1.5" aria-hidden="true">
                <span class={`h-[3px] flex-1 rounded-pill transition-colors ${pwStrength() >= 1 ? (pwStrength() === 1 ? 'bg-error' : pwStrength() === 2 ? 'bg-warning' : 'bg-success') : 'bg-surface-tertiary'}`} />
                <span class={`h-[3px] flex-1 rounded-pill transition-colors ${pwStrength() >= 2 ? (pwStrength() === 2 ? 'bg-warning' : 'bg-success') : 'bg-surface-tertiary'}`} />
                <span class={`h-[3px] flex-1 rounded-pill transition-colors ${pwStrength() >= 3 ? 'bg-success' : 'bg-surface-tertiary'}`} />
              </div>
              <p class="mt-1.5 text-[11.5px] text-content-tertiary">已开启 IP 限流 · 多次失败递增冷却（最大 {ADMIN_MAX_LOCK_WAIT_SECS} s）</p>
            </div>

            {/* 30 天保持登录 —— 勾选时 login 携带 rememberMe,后端签发 30 天 token(否则默认 2h) */}
            <label class="flex items-center gap-2 my-1.5 text-[12.5px] text-content-secondary cursor-pointer select-none">
              <input
                type="checkbox"
                checked={rememberMe()}
                disabled={loading() || lockSeconds() > 0}
                onChange={(e) => setRememberMe(e.currentTarget.checked)}
                class="size-3.5 rounded border-border text-accent focus:ring-2 focus:ring-accent/30 disabled:opacity-60"
              />
              30 天保持登录 · 仅限本设备
            </label>

            <button
              type="submit"
              disabled={loading() || lockSeconds() > 0}
              class="w-full h-10 rounded-md bg-accent text-white text-[13.5px] font-semibold inline-flex items-center justify-center gap-1.5 hover:bg-accent-hover active:bg-accent-active focus-ring transition disabled:opacity-60 disabled:cursor-not-allowed"
            >
              <Show when={!loading()} fallback={
                <>
                  <span class="size-3.5 border-2 border-white/40 border-t-white rounded-full animate-spin" />
                  正在验证…
                </>
              }>
                {lockSeconds() > 0 ? `锁定中 ${lockSeconds()}s` : '进入管理后台'}
                <Show when={lockSeconds() === 0}>
                  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M5 12h14M13 6l6 6-6 6" />
                  </svg>
                </Show>
              </Show>
            </button>
          </form>

          {/* setup hint */}
          <div class="mt-4 flex items-start gap-2 px-3.5 py-3 rounded-md bg-surface-secondary text-[12.5px] text-content-secondary">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="text-accent shrink-0 mt-0.5">
              <circle cx="12" cy="12" r="10" /><path d="M12 16v-4" /><path d="M12 8h.01" />
            </svg>
            <span>
              首次部署？请访问{' '}
              <a href="/admin/setup" class="text-accent hover:underline">/admin/setup</a>{' '}
              初始化首位管理员账号。
            </span>
          </div>
        </div>

        {/* footer */}
        <div class="flex items-center justify-between gap-3 pt-4 border-t border-border-hairline text-[11.5px] text-content-tertiary">
          <span class="inline-flex items-center gap-1.5">
            <span
              class={`size-1.5 rounded-full ${publicHealth() ? 'bg-success animate-ring-pulse' : 'bg-content-quaternary'}`}
              aria-hidden="true"
            />
            <span>
              API · {publicHealth() ? '健康' : publicHealth.loading ? '检查中' : '未知'} · <span class="font-mono tabular-nums">{clock()}</span>
            </span>
          </span>
          <div class="flex items-center gap-3">
            {/* app version：来自 /health 公开端点(CARGO_PKG_VERSION) */}
            <span class="font-mono">
              {appVersion() ? `v${appVersion()}` : `build ${import.meta.env.MODE === 'production' ? 'prod' : 'dev'}`}
            </span>
          </div>
        </div>
      </div>

      {/* 右栏 —— 装饰 */}
      <div
        class="relative hidden md:flex flex-col p-11 text-white overflow-hidden min-h-screen"
        style={{ background: 'linear-gradient(160deg, oklch(22% 0.06 269) 0%, oklch(14% 0.04 264) 100%)' }}
        aria-hidden="true"
      >
        {/* 双 blur 光晕 */}
        <div
          class="absolute pointer-events-none"
          style={{
            width: '700px', height: '700px', right: '-260px', top: '-260px',
            background: 'radial-gradient(closest-side, color-mix(in oklab, var(--accent) 50%, transparent), transparent)',
            filter: 'blur(40px)',
          }}
        />
        <div
          class="absolute pointer-events-none"
          style={{
            width: '600px', height: '600px', left: '-200px', bottom: '-300px',
            background: 'radial-gradient(closest-side, color-mix(in oklab, oklch(56% 0.22 300) 45%, transparent), transparent)',
            filter: 'blur(40px)',
          }}
        />

        <div class="relative z-10 flex flex-col h-full">
          <span class="self-start inline-flex px-2.5 py-1 rounded-pill border border-white/12 bg-white/[0.08] text-[11.5px] font-semibold tracking-[0.05em]">
            AMAS · ADAPTIVE MASTERY ACQUISITION
          </span>
          <h2 class="mt-5 text-[36px] leading-[1.12] font-bold tracking-[-0.024em] max-w-[18ch]">
            每一次答题，引擎都在重新调度未来的词。
          </h2>
          <p class="mt-2.5 text-white/70 text-[14.5px] leading-[1.55] max-w-[38ch]">
            24 个 AMAS 子配置 · 8 算法 flag（4 决策 + 4 记忆模型）· ELO + 疲劳信号融合。本管理后台是 AMAS 的指挥台。
          </p>

          <ul class="list-none p-0 mt-7 flex flex-col gap-2.5">
            {[
              '配置热加载 · 500ms 防抖 · 原子生效',
              '自更新 · GitHub Release · VACUUM INTO 备份',
              '21 个内部 worker · 实时心跳 + 告警',
            ].map((t) => (
              <li class="flex items-start gap-2.5 text-[13px] text-white/[0.78]">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" class="shrink-0 mt-0.5" style={{ color: 'oklch(78% 0.16 162)' }}>
                  <polyline points="20 6 9 17 4 12" />
                </svg>
                <span>{t}</span>
              </li>
            ))}
          </ul>

          {/* health-card */}
          <div
            class="mt-auto grid grid-cols-3 gap-[18px] rounded-xl p-5"
            style={{
              background: 'rgba(255,255,255,0.06)',
              border: '1px solid rgba(255,255,255,0.10)',
              'backdrop-filter': 'blur(10px)',
            }}
          >
            <div class="flex flex-col gap-1">
              <span class="text-[10.5px] uppercase tracking-[0.06em]" style={{ color: 'rgba(255,255,255,0.5)' }}>UPTIME</span>
              <span class="text-[17px] font-semibold tabular-nums" style={{ color: uptimeSecs() != null ? 'oklch(78% 0.13 162)' : 'rgba(255,255,255,0.7)' }}>
                {formatUptime(uptimeSecs())}
              </span>
            </div>
            <div class="flex flex-col gap-1">
              <span class="text-[10.5px] uppercase tracking-[0.06em]" style={{ color: 'rgba(255,255,255,0.5)' }}>{sloLabel()}</span>
              <span
                class="text-[17px] font-semibold tabular-nums"
                style={{ color: sloPct() != null ? 'oklch(78% 0.13 162)' : 'rgba(255,255,255,0.7)' }}
                title={sloPct() != null ? `真实可用性(${availability()!.totalRequests} 次请求样本)` : '暂无请求样本'}
              >
                {sloPct() != null ? `${sloPct()!.toFixed(2)}%` : '—'}
              </span>
            </div>
            <div class="flex flex-col gap-1">
              <span class="text-[10.5px] uppercase tracking-[0.06em]" style={{ color: 'rgba(255,255,255,0.5)' }}>DB SIZE</span>
              <span class="text-[17px] font-semibold tabular-nums" style={{ color: 'rgba(255,255,255,0.7)' }}>
                {formatBytes(dbSizeBytes())}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
