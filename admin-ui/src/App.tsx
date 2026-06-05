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

// 路由级错误边界：单页资源请求失败时只降级该页内容区，不冒泡到全局 AppErrorBoundary
// 把整个 admin 壳（侧栏/导航）替换掉。参照 pages/amas/PanelBoundary.tsx，用通用样式。
function RouteBoundary(props: { children: JSX.Element }) {
  return (
    <ErrorBoundary
      fallback={(err, reset) => (
        <div class="flex items-center justify-center min-h-[60vh] p-4">
          <Card variant="elevated" class="max-w-md w-full">
            <Empty
              title="该页面加载失败"
              description={err?.message ? String(err.message) : String(err)}
              action={<Button onClick={reset}>重试</Button>}
            />
          </Card>
        </div>
      )}
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
