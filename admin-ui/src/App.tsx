import { Router, Route, useLocation, type RouteSectionProps } from '@solidjs/router';
import { lazy, Suspense, Show, ErrorBoundary, onMount, onCleanup, type JSX, type Component } from 'solid-js';
import { Toaster } from '@/components/ui/Toast';
import { AppErrorBoundary } from '@/components/ErrorBoundary';
import { AdminLayout } from '@/components/layout/AdminLayout';
import { AdminProtectedRoute } from '@/components/auth/ProtectedRoute';
import { Spinner } from '@/components/ui/Spinner';
import { Card } from '@/components/ui/Card';
import { Empty } from '@/components/ui/Empty';
import { Button } from '@/components/ui/Button';
import { api, maintenanceActive, setMaintenanceActive, setUpdateInfo } from '@/api/http';
import MaintenancePage from '@/pages/MaintenancePage';
import { UpdateBanner } from '@/components/ui/UpdateBanner';
import { startProbeBridge, stopProbeBridge } from '@/workers/probe/api-bridge';
import { installRingBuffers } from '@/workers/probe/ring-buffers';

// bundle 启动时立刻注册环形 buffer（不依赖 onMount，要尽早覆盖 console / error / fetch）。
installRingBuffers();

const NotFoundPage = lazy(() => import('@/pages/NotFoundPage'));
const LegacyUserFrontendPage = lazy(() => import('@/pages/LegacyUserFrontendPage'));

const LoginPage = lazy(() => import('@/pages/LoginPage'));
const SetupPage = lazy(() => import('@/pages/SetupPage'));
const DashboardPage = lazy(() => import('@/pages/DashboardPage'));
const UserManagementPage = lazy(() => import('@/pages/UserManagementPage'));
const AmasConfigPage = lazy(() => import('@/pages/AmasConfigPage'));
const AmasMetricsPage = lazy(() => import('@/pages/AmasMetricsPage'));
const AmasAdvisorPage = lazy(() => import('@/pages/AmasAdvisorPage'));
const AmasExperimentsPage = lazy(() => import('@/pages/AmasExperimentsPage'));
const MonitoringPage = lazy(() => import('@/pages/MonitoringPage'));
const AnalyticsPage = lazy(() => import('@/pages/AnalyticsPage'));
const SettingsPage = lazy(() => import('@/pages/SettingsPage'));
const UpdatesPage = lazy(() => import('@/pages/UpdatesPage'));
const WordbookCenterPage = lazy(() => import('@/pages/WordbookCenterPage'));
const DevicesPage = lazy(() => import('@/pages/DevicesPage'));
const FeedbackPage = lazy(() => import('@/pages/FeedbackPage'));
const ProbePage = lazy(() => import('@/pages/ProbePage'));
const ProbeMetricsPage = lazy(() => import('@/pages/ProbeMetricsPage'));
const BroadcastPage = lazy(() => import('@/pages/BroadcastPage'));
const ResourcePacksPage = lazy(() => import('@/pages/ResourcePacksPage'));

function PageSpinner() {
  return (
    <div class="flex items-center justify-center min-h-[60vh]">
      <Spinner size="lg" />
    </div>
  );
}

// 加载类错误判定：懒加载 chunk 失败 / 模块脚本失败 / 网关 502·503·504。
// 多见于服务端升级重启——旧 SPA 引用的旧 hash chunk 已被新构建替换、或重启窗口内反代 502。
// 此时 reset() 只重渲染会重取同一旧 chunk，无效；须轮询服务端，恢复后整页重载取新 manifest。
function isLoadError(err: unknown): boolean {
  const m = err instanceof Error ? (err.message || '') : String(err ?? '');
  return /dynamically imported module|module script|Loading chunk|chunk load|Failed to fetch|Bad Gateway|50[234]\b/i.test(m);
}

// 升级 / 重启导致的加载失败：保留 admin 壳层，轮询 /api/status，服务端回来即整页重载。
function ReloadOnRecover() {
  let timer: ReturnType<typeof setInterval> | undefined;
  const reloadGuarded = () => {
    try {
      const now = Date.now();
      const recent = (JSON.parse(sessionStorage.getItem('wf_reload_guard') || '[]') as number[]).filter((t) => now - t < 60_000);
      if (recent.length >= 3) return; // 1 分钟内已重载 3 次则停手，避免重载循环，保留手动按钮（冷却后可再恢复）
      recent.push(now);
      sessionStorage.setItem('wf_reload_guard', JSON.stringify(recent));
    } catch { /* sessionStorage 不可用时直接重载 */ }
    window.location.reload();
  };
  onMount(() => {
    timer = setInterval(() => {
      api.get('/api/status').then(reloadGuarded).catch(() => { /* 仍在重启，继续轮询 */ });
    }, 3000);
  });
  onCleanup(() => { if (timer) clearInterval(timer); });
  return (
    <div class="flex items-center justify-center min-h-[60vh] p-4">
      <Card variant="elevated" class="max-w-md w-full">
        <Empty
          title="页面更新中"
          description="服务正在重启或已发布新版本，正在自动重连…"
          action={<Button onClick={() => window.location.reload()}>立即重试</Button>}
        />
      </Card>
    </div>
  );
}

// 路由级错误边界：单页资源请求失败时只降级该页内容区，不冒泡到全局 AppErrorBoundary
// 把整个 admin 壳（侧栏/导航）替换掉。加载类错误（升级重启 / chunk 失效）走自动重连恢复。
function RouteBoundary(props: { children: JSX.Element }) {
  return (
    <ErrorBoundary
      fallback={(err, reset) =>
        isLoadError(err) ? (
          <ReloadOnRecover />
        ) : (
          <div class="flex items-center justify-center min-h-[60vh] p-4">
            <Card variant="elevated" class="max-w-md w-full">
              <Empty
                title="该页面加载失败"
                description={err?.message ? String(err.message) : String(err)}
                action={<Button onClick={reset}>重试</Button>}
              />
            </Card>
          </div>
        )
      }
    >
      {props.children}
    </ErrorBoundary>
  );
}

// 公开页（login/setup/legacy/notfound）：边界 → Suspense → 页面
const pub_ = (Page: Component) => () => (
  <RouteBoundary><Suspense fallback={<PageSpinner />}><Page /></Suspense></RouteBoundary>
);

// 受保护 admin 页：边界 → 鉴权守卫 → Suspense → 页面
const guarded = (Page: Component) => () => (
  <RouteBoundary><AdminProtectedRoute><Suspense fallback={<PageSpinner />}><Page /></Suspense></AdminProtectedRoute></RouteBoundary>
);

function MaintenanceProvider(props: RouteSectionProps) {
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let initialVersion: string | undefined;
  const location = useLocation();
  const isAdminPath = () => location.pathname.startsWith('/admin');

  const checkStatus = async () => {
    try {
      const data = await api.get<{ maintenanceMode: boolean; version?: string }>('/api/status');
      setMaintenanceActive(data.maintenanceMode);
      if (data.version) {
        if (!initialVersion) {
          initialVersion = data.version;
        } else if (initialVersion !== data.version) {
          setUpdateInfo({ version: data.version, message: '有新版本可用，请刷新页面获取最新内容' });
          initialVersion = data.version;
        }
      }
    } catch {
      // ignore
    }
  };

  onMount(() => {
    checkStatus();
    pollTimer = setInterval(checkStatus, 30_000);
    // 远程探针：客户端 worker bridge 全局只启动一次；connectSseStream 内部按 token
    // 自适应（无 token → 401 → 自动重试，admin 登录后会自然恢复）。
    startProbeBridge();
  });

  onCleanup(() => {
    if (pollTimer) clearInterval(pollTimer);
    stopProbeBridge();
  });

  return (
    <>
      <UpdateBanner />
      <Show when={!maintenanceActive() || isAdminPath()} fallback={<MaintenancePage />}>
        {props.children}
      </Show>
    </>
  );
}

export default function App() {
  return (
    <AppErrorBoundary>
      <Router root={MaintenanceProvider}>
        <Route path="/" component={() => { window.location.replace('/admin/login'); return null; }} />
        <Route path="/login" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/register" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/learning" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/flashcard" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/vocabulary" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/wordbooks" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/wordbook-center" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/statistics" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/history" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/profile" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/notifications" component={pub_(LegacyUserFrontendPage)} />
        <Route path="/admin">
          <Route path="/login" component={pub_(LoginPage)} />
          <Route path="/setup" component={pub_(SetupPage)} />
          <Route path="/" component={AdminLayout}>
            <Route path="/" component={guarded(DashboardPage)} />
            <Route path="/users" component={guarded(UserManagementPage)} />
            <Route path="/clients" component={guarded(DevicesPage)} />
            <Route path="/amas-config" component={guarded(AmasConfigPage)} />
            <Route path="/amas-metrics" component={guarded(AmasMetricsPage)} />
            <Route path="/amas-advisor" component={guarded(AmasAdvisorPage)} />
            <Route path="/amas-experiments" component={guarded(AmasExperimentsPage)} />
            <Route path="/monitoring" component={guarded(MonitoringPage)} />
            <Route path="/analytics" component={guarded(AnalyticsPage)} />
            <Route path="/settings" component={guarded(SettingsPage)} />
            <Route path="/updates" component={guarded(UpdatesPage)} />
            <Route path="/wordbook-center" component={guarded(WordbookCenterPage)} />
            <Route path="/feedback" component={guarded(FeedbackPage)} />
            <Route path="/probe" component={guarded(ProbeMetricsPage)} />
            <Route path="/remote-probe" component={guarded(ProbePage)} />
            <Route path="/broadcast" component={guarded(BroadcastPage)} />
            <Route path="/resource-packs" component={guarded(ResourcePacksPage)} />
          </Route>
          <Route path="*" component={pub_(NotFoundPage)} />
        </Route>
        <Route path="*" component={pub_(NotFoundPage)} />
      </Router>
      <Toaster />
    </AppErrorBoundary>
  );
}
