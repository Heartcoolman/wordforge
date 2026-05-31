import { createSignal, createMemo, onMount, onCleanup, Show, For } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { adminApi, type EnvCheckItem } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { uiStore } from '@/stores/ui';
import { MIN_PASSWORD_LENGTH } from '@/lib/constants';

/**
 * 初始化向导。对齐 setup.html：
 *   ① 环境自检面板（5 项）— GET /setup/env-check
 *   ② 创建超管 — 邮箱 + 密码强度计（4 档纯前端）+ 二次确认
 *   ③ 成功态 — 3 秒倒计时自动跳转
 * 左侧 hero + 自检 + "创建成功后会发生什么"，右侧表单卡（进度条/表单/安全脚注/成功态）。
 */

type Step = 1 | 2 | 3;
const STEP_LABELS: Record<Step, string> = { 1: '检测', 2: '创建超管', 3: '登录' };

/** 创建成功后会发生什么 */
const NEXT_UP = [
  '本页面后续不再可访问（initialized=true 后重定向 login）',
  '自动写入 JWT 并跳转 /admin',
  '21 个后台 worker 默认全部启用，可在监控页关掉不需要的',
  '资源包 / 词库需要管理员手动上传，初始为空',
];

/** 4 档密码强度（纯前端，对齐设计图 strength()） */
function strength(p: string): { l: 0 | 1 | 2 | 3 | 4; t: string } {
  if (!p) return { l: 0, t: '先输入密码' };
  if (p.length < MIN_PASSWORD_LENGTH) return { l: 1, t: `太短 · 至少 ${MIN_PASSWORD_LENGTH} 位` };
  let s = 1;
  if (/[a-z]/.test(p) && /[A-Z]/.test(p)) s++;
  if (/[0-9]/.test(p)) s++;
  if (/[^a-zA-Z0-9]/.test(p) || p.length >= 14) s++;
  const labels = ['弱', '一般', '良好', '强'];
  return { l: s as 1 | 2 | 3 | 4, t: '强度：' + labels[s - 1] };
}

function validateEmail(v: string) {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v);
}

/** 自检图标 + 配色：ok=绿√、检查中=spinner、失败=红×、pending=灰虚线 */
function StatusIc(props: { state: 'ok' | 'checking' | 'fail' }) {
  return (
    <Show
      when={props.state !== 'checking'}
      fallback={<span class="size-[18px] rounded-full border-2 border-accent border-r-transparent animate-spin" />}
    >
      <span
        class={
          'size-[18px] rounded-full grid place-items-center text-white ' +
          (props.state === 'ok' ? 'bg-success' : 'bg-error')
        }
      >
        <Show
          when={props.state === 'ok'}
          fallback={
            <svg class="size-[11px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          }
        >
          <svg class="size-[11px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        </Show>
      </span>
    </Show>
  );
}

export default function SetupPage() {
  const navigate = useNavigate();
  const [step, setStep] = createSignal<Step>(1);

  // 环境自检
  const [checking, setChecking] = createSignal(true);
  const [checks, setChecks] = createSignal<EnvCheckItem[]>([]);
  const [checkError, setCheckError] = createSignal('');

  // 表单
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [confirm, setConfirm] = createSignal('');
  const [touched, setTouched] = createSignal({ email: false, confirm: false });
  const [loading, setLoading] = createSignal(false);
  const [formError, setFormError] = createSignal('');

  // 成功态倒计时
  const [countdown, setCountdown] = createSignal(3);
  let cdTimer: ReturnType<typeof setInterval> | undefined;
  onCleanup(() => cdTimer && clearInterval(cdTimer));

  async function loadEnvCheck() {
    setChecking(true);
    setCheckError('');
    try {
      const status = await adminApi.checkStatus();
      if (status.initialized) {
        navigate('/admin/login', { replace: true });
        return;
      }
      const res = await adminApi.setupEnvCheck();
      setChecks(res.checks);
    } catch (err: unknown) {
      setCheckError(err instanceof Error ? err.message : '无法连接到服务器，请检查后端是否运行');
    } finally {
      setChecking(false);
    }
  }
  onMount(loadEnvCheck);

  const pwStrength = createMemo(() => strength(password()));
  const emailOk = createMemo(() => validateEmail(email()));
  const pwOk = createMemo(() => password().length >= MIN_PASSWORD_LENGTH);
  const confirmOk = createMemo(() => confirm() === password() && confirm().length > 0);
  const canSubmit = createMemo(() => emailOk() && pwOk() && confirmOk() && !loading());

  async function handleSubmit(e: Event) {
    e.preventDefault();
    setFormError('');
    if (!canSubmit()) return;
    setLoading(true);
    try {
      const res = await adminApi.setup({ email: email(), password: password() });
      setPassword('');
      setConfirm('');
      tokenManager.setAdminToken(res.token);
      uiStore.toast.success('管理员账户已创建');
      setStep(3);
      // 3 秒倒计时自动跳转
      cdTimer = setInterval(() => {
        setCountdown((c) => {
          if (c <= 1) {
            clearInterval(cdTimer);
            navigate('/admin', { replace: true });
            return 0;
          }
          return c - 1;
        });
      }, 1000);
    } catch (err: unknown) {
      setFormError(err instanceof Error ? err.message : '创建失败');
    } finally {
      setLoading(false);
    }
  }

  /** 进度条单步状态 */
  const stepCls = (target: Step) => {
    const cur = step();
    const base = 'inline-flex items-center gap-1 px-2.5 py-1 rounded-md font-mono text-[10.5px] uppercase tracking-[0.06em]';
    if (target < cur) return base + ' bg-success-light text-success-strong';
    if (target === cur) return base + ' bg-accent-light text-accent font-semibold';
    return base + ' text-content-tertiary';
  };

  return (
    <div class="relative min-h-screen grid place-items-center p-6 overflow-x-hidden bg-surface-secondary">
      <div aria-hidden="true" class="absolute inset-0 bg-gradient-accent-soft pointer-events-none" />
      <div aria-hidden="true" class="absolute -top-32 -left-32 size-96 rounded-full bg-accent/10 blur-3xl pointer-events-none" />
      <div aria-hidden="true" class="absolute -bottom-32 -right-32 size-96 rounded-full bg-info/10 blur-3xl pointer-events-none" />

      <div class="relative w-full max-w-[1080px] grid gap-8 items-start grid-cols-1 lg:grid-cols-[1fr_460px]">

        {/* ===== 左：hero + 环境自检 + next-up ===== */}
        <div class="pt-9 px-2 animate-fade-in-up">
          <div class="size-[52px] rounded-[14px] bg-content text-surface grid place-items-center font-display font-extrabold text-[22px] tracking-[-0.03em] mb-[22px]">W</div>
          <div class="font-mono font-semibold text-[11.5px] tracking-[0.16em] uppercase text-accent mb-4">SETUP · FIRST RUN</div>
          <h1 class="font-display font-bold text-[38px] leading-[1.12] tracking-[-0.026em] text-content mb-3 max-w-[12ch]">初始化 WordForge 管理后台</h1>
          <p class="text-[14.5px] leading-[1.55] text-content-secondary max-w-[38ch] mb-7">检测到该实例尚未创建管理员账户。请创建超级管理员帐号 — 后续会自动登录并跳转到管理后台。</p>

          {/* 环境自检面板 */}
          <Card variant="elevated" padding="none" class="overflow-hidden border border-border-hairline">
            <div class="flex items-baseline justify-between px-[18px] py-3.5 border-b border-border-hairline">
              <h3 class="font-display font-semibold text-[12.5px]">环境自检</h3>
              <span class="font-mono font-medium text-[10.5px] tracking-[0.04em] uppercase text-content-tertiary">/setup/env-check</span>
            </div>
            <div class="py-1.5">
              <Show
                when={!checking()}
                fallback={
                  <div class="flex flex-col items-center gap-2 py-8">
                    <Spinner size="lg" />
                    <p class="text-caption">检测环境…</p>
                  </div>
                }
              >
                <Show
                  when={!checkError()}
                  fallback={
                    <div class="flex flex-col items-center gap-3 px-[18px] py-6 text-center">
                      <p class="text-sm text-error-strong">{checkError()}</p>
                      <Button variant="outline" size="sm" onClick={loadEnvCheck}>重试</Button>
                    </div>
                  }
                >
                  <For each={checks()}>
                    {(item) => (
                      <div class="grid grid-cols-[22px_1fr_auto] gap-3 items-center px-[18px] py-2.5 transition-colors hover:bg-surface-secondary">
                        <StatusIc state={item.ok ? 'ok' : 'fail'} />
                        <div>
                          <span class={'font-display font-medium text-[13px] leading-[1.3] ' + (item.ok ? 'text-content' : 'text-error-strong')}>{item.label}</span>
                          <span class="block mt-0.5 font-mono text-[11.5px] leading-[1.4] text-content-tertiary">{item.detail}</span>
                        </div>
                        <span class={'font-mono font-medium text-[11.5px] tabular-nums ' + (item.ok ? 'text-success-strong' : 'text-error-strong')}>{item.ok ? 'OK' : '失败'}</span>
                      </div>
                    )}
                  </For>
                </Show>
              </Show>
            </div>
          </Card>

          {/* 创建成功后会发生什么 */}
          <div class="mt-3.5 px-4 py-3 bg-accent-light border-l-[3px] border-accent rounded-sm text-[12px] leading-[1.55] text-content-secondary">
            <strong class="text-content font-semibold">创建成功后会发生什么</strong>
            <ul class="list-none p-0 mt-2 flex flex-col gap-1">
              <For each={NEXT_UP}>
                {(t) => <li class="relative pl-4 text-[11.5px] before:content-['→'] before:absolute before:left-0 before:text-accent before:font-semibold">{t}</li>}
              </For>
            </ul>
          </div>
        </div>

        {/* ===== 右：表单卡 ===== */}
        <Card variant="elevated" padding="none" class="overflow-hidden rounded-[20px] shadow-elevation-3 border border-border-hairline animate-fade-in-up">
          {/* 进度条 */}
          <div class="flex gap-1.5 items-center px-3 py-2 bg-surface border-b border-border-hairline">
            <For each={[1, 2, 3] as Step[]}>
              {(s, i) => (
                <>
                  <Show when={i() > 0}><span class="text-content-quaternary text-[10px]">→</span></Show>
                  <span class={stepCls(s)}>
                    <Show when={s < step()}>
                      <svg class="size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12" /></svg>
                    </Show>
                    {STEP_LABELS[s]}
                  </span>
                </>
              )}
            </For>
          </div>

          {/* 表单（step 1/2） */}
          <Show when={step() !== 3}>
            <div class="px-8 pt-[26px] pb-2">
              <h2 class="font-display font-semibold text-[20px] leading-[1.2] tracking-[-0.018em] text-content mb-1">创建超级管理员</h2>
              <p class="text-[13px] leading-[1.55] text-content-secondary">这个账户拥有全部 admin 权限，包括用户管理、AMAS 调参、资源包发布、广播、自更新。请妥善保管。</p>
            </div>
            <form onSubmit={handleSubmit} class="px-8 pt-[22px] pb-7 flex flex-col gap-[18px]" autocomplete="off">
              <Show when={formError()}>
                <div role="alert" class="rounded-md border border-error/30 bg-error-light px-3 py-2 text-sm text-error animate-fade-in">{formError()}</div>
              </Show>

              <Input
                label="管理员邮箱 *"
                type="email"
                required
                autocomplete="email"
                placeholder="admin@example.com"
                value={email()}
                onInput={(e) => setEmail(e.currentTarget.value)}
                onBlur={() => setTouched((t) => ({ ...t, email: true }))}
                error={touched().email && email() && !emailOk() ? '请输入合法邮箱地址' : undefined}
              />

              {/* 密码 + 强度计 */}
              <div class="flex flex-col gap-1.5">
                <Input
                  label="密码 *"
                  type="password"
                  required
                  autocomplete="new-password"
                  placeholder={`至少 ${MIN_PASSWORD_LENGTH} 位，包含字母与数字`}
                  value={password()}
                  onInput={(e) => setPassword(e.currentTarget.value)}
                />
                <div class="grid grid-cols-4 gap-1 mt-1.5">
                  <For each={[1, 2, 3, 4]}>
                    {(seg) => (
                      <span
                        class={
                          'h-1 rounded-full transition-colors duration-300 ' +
                          (pwStrength().l < seg
                            ? 'bg-surface-tertiary'
                            : pwStrength().l === 1
                              ? 'bg-error'
                              : pwStrength().l === 2
                                ? 'bg-warning'
                                : pwStrength().l === 3
                                  ? 'bg-info-strong'
                                  : 'bg-success')
                        }
                      />
                    )}
                  </For>
                </div>
                <div
                  class={
                    'font-mono font-medium text-[11px] leading-[1.4] mt-0.5 min-h-[14px] ' +
                    (pwStrength().l === 1
                      ? 'text-error-strong'
                      : pwStrength().l === 2
                        ? 'text-warning-strong'
                        : pwStrength().l === 3
                          ? 'text-info-strong'
                          : pwStrength().l === 4
                            ? 'text-success-strong'
                            : 'text-content-tertiary')
                  }
                >
                  {pwStrength().t}
                </div>
              </div>

              <Input
                label="确认密码 *"
                type="password"
                required
                autocomplete="new-password"
                placeholder="再输入一次"
                value={confirm()}
                onInput={(e) => setConfirm(e.currentTarget.value)}
                onBlur={() => setTouched((t) => ({ ...t, confirm: true }))}
                error={touched().confirm && confirm() && !confirmOk() ? '两次输入不一致' : undefined}
              />

              <Button type="submit" fullWidth loading={loading()} disabled={!canSubmit()}>
                创建管理员并登录
              </Button>
            </form>

            {/* 安全脚注 */}
            <div class="flex gap-1.5 items-center px-8 py-3.5 pb-[22px] border-t border-border-hairline bg-surface-secondary font-mono font-medium text-[11.5px] leading-[1.55] text-content-tertiary">
              <svg class="size-3 text-accent shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /></svg>
              <span>
                密码经 argon2id 散列存储 · 不写 cookie，仅返回 JWT · 调用{' '}
                <code class="font-mono text-[11px] bg-surface px-1.5 py-px rounded text-accent">POST /api/admin/setup</code>
              </span>
            </div>
          </Show>

          {/* 成功态 + 3 秒倒计时 */}
          <Show when={step() === 3}>
            <div class="px-8 pt-9 pb-7 text-center animate-fade-in-up">
              <div class="flex gap-1.5 items-center justify-center pb-[18px]">
                <span class={stepCls(1)}>
                  <svg class="size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12" /></svg>
                  {STEP_LABELS[1]}
                </span>
                <span class="text-content-quaternary text-[10px]">→</span>
                <span class={stepCls(2)}>
                  <svg class="size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12" /></svg>
                  {STEP_LABELS[2]}
                </span>
                <span class="text-content-quaternary text-[10px]">→</span>
                <span class={stepCls(3)}>{STEP_LABELS[3]}</span>
              </div>
              <div class="size-16 rounded-full bg-success text-white grid place-items-center mx-auto mb-[18px] animate-scale-in">
                <svg class="size-[30px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3"><polyline points="20 6 9 17 4 12" /></svg>
              </div>
              <h2 class="font-display font-semibold text-[20px] leading-[1.2] mb-1.5">管理员账户已创建</h2>
              <p class="text-[13px] text-content-secondary mb-[22px]">JWT 已签发，正在跳转管理后台…</p>
              <div class="font-mono font-medium text-[11.5px] text-content-tertiary">
                即将跳转 <strong class="text-accent">{countdown()}</strong> 秒后
              </div>
              <Button class="mt-4" variant="ghost" size="sm" onClick={() => navigate('/admin', { replace: true })}>立即进入</Button>
            </div>
          </Show>
        </Card>
      </div>
    </div>
  );
}
