import { Router, Route, useLocation } from '@solidjs/router';
import { lazy, Suspense, Show, onMount, onCleanup } from 'solid-js';
import { Toaster } from '@/components/ui/Toast';
import { AppErrorBoundary } from '@/components/ErrorBoundary';
import { AdminLayout } from '@/components/layout/AdminLayout';
import { AdminProtectedRoute } from '@/components/auth/ProtectedRoute';
import { Spinner } from '@/components/ui/Spinner';
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

function PageSpinner() {
  return (
    <div class="flex items-center justify-center min-h-[60vh]">
      <Spinner size="lg" />
    </div>
  );
}

function MaintenanceProvider(props: { children: any }) {
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
        <Route path="/login" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/register" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/learning" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/flashcard" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/vocabulary" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/wordbooks" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/wordbook-center" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/statistics" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/history" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/profile" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/notifications" component={() => (<Suspense fallback={<PageSpinner />}><LegacyUserFrontendPage /></Suspense>)} />
        <Route path="/admin">
          <Route path="/login" component={() => (<Suspense fallback={<PageSpinner />}><LoginPage /></Suspense>)} />
          <Route path="/setup" component={() => (<Suspense fallback={<PageSpinner />}><SetupPage /></Suspense>)} />
          <Route path="/" component={AdminLayout}>
            <Route path="/" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><DashboardPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/users" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><UserManagementPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/clients" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><DevicesPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/amas-config" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AmasConfigPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/amas-metrics" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AmasMetricsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/amas-advisor" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AmasAdvisorPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/monitoring" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><MonitoringPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/analytics" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AnalyticsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/settings" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><SettingsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/updates" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><UpdatesPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/wordbook-center" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><WordbookCenterPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/feedback" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><FeedbackPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/probe" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><ProbePage /></Suspense></AdminProtectedRoute>)} />
          </Route>
          <Route path="*" component={() => (<Suspense fallback={<PageSpinner />}><NotFoundPage /></Suspense>)} />
        </Route>
        <Route path="*" component={() => (<Suspense fallback={<PageSpinner />}><NotFoundPage /></Suspense>)} />
      </Router>
      <Toaster />
    </AppErrorBoundary>
  );
}
