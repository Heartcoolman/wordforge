import { Router, Route } from '@solidjs/router';
import { lazy, Suspense, Show, createSignal, onMount, onCleanup } from 'solid-js';
import { Toaster } from '@/components/ui/Toast';
import { AppErrorBoundary } from '@/components/ErrorBoundary';
import { AdminLayout } from '@/components/layout/AdminLayout';
import { AdminProtectedRoute } from '@/components/auth/ProtectedRoute';
import { Spinner } from '@/components/ui/Spinner';
import { api, maintenanceActive, setMaintenanceActive, setUpdateInfo, connectSseStream } from '@/api/client';
import { startTelemetryWorker, stopTelemetryWorker, handleTelemetryRequest } from '@/workers/telemetry';
import MaintenancePage from '@/pages/MaintenancePage';
import { UpdateBanner } from '@/components/ui/UpdateBanner';
import { SystemLockedModal } from '@/components/SystemLockedModal';
import { Portal } from 'solid-js/web';

const NotFoundPage = lazy(() => import('@/pages/NotFoundPage'));

const AdminLoginPage = lazy(() => import('@/pages/admin/AdminLoginPage'));
const AdminSetupPage = lazy(() => import('@/pages/admin/AdminSetupPage'));
const AdminDashboard = lazy(() => import('@/pages/admin/AdminDashboard'));
const UserManagementPage = lazy(() => import('@/pages/admin/UserManagementPage'));
const AmasConfigPage = lazy(() => import('@/pages/admin/AmasConfigPage'));
const MonitoringPage = lazy(() => import('@/pages/admin/MonitoringPage'));
const AnalyticsPage = lazy(() => import('@/pages/admin/AnalyticsPage'));
const SettingsPage = lazy(() => import('@/pages/admin/SettingsPage'));
const AdminWordbookCenterPage = lazy(() => import('@/pages/admin/AdminWordbookCenterPage'));
const ClientsPage = lazy(() => import('@/pages/admin/ClientsPage'));

function PageSpinner() {
  return (
    <div class="flex items-center justify-center min-h-[60vh]">
      <Spinner size="lg" />
    </div>
  );
}

const [systemLocked, setSystemLocked] = createSignal(false);

function MaintenanceProvider(props: { children: any }) {
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let disconnectSse: (() => void) | undefined;
  let initialVersion: string | undefined;

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
    startTelemetryWorker();
    disconnectSse = connectSseStream({
      onTelemetryRequest: handleTelemetryRequest,
      onDataCorrupted: () => setSystemLocked(true),
    });
  });

  onCleanup(() => {
    if (pollTimer) clearInterval(pollTimer);
    stopTelemetryWorker();
    disconnectSse?.();
  });

  return (
    <>
      <UpdateBanner />
      <Show when={!maintenanceActive()} fallback={<MaintenancePage />}>
        {props.children}
      </Show>
    </>
  );
}

export default function App() {
  return (
    <AppErrorBoundary>
      <Show when={systemLocked()}>
        <Portal mount={document.body}>
          <SystemLockedModal />
        </Portal>
      </Show>
      <MaintenanceProvider>
      <Router>
        <Route path="/admin">
          <Route path="/login" component={() => (<Suspense fallback={<PageSpinner />}><AdminLoginPage /></Suspense>)} />
          <Route path="/setup" component={() => (<Suspense fallback={<PageSpinner />}><AdminSetupPage /></Suspense>)} />
          <Route path="/" component={AdminLayout}>
            <Route path="/" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AdminDashboard /></Suspense></AdminProtectedRoute>)} />
            <Route path="/users" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><UserManagementPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/clients" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><ClientsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/amas-config" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AmasConfigPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/monitoring" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><MonitoringPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/analytics" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AnalyticsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/settings" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><SettingsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/wordbook-center" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AdminWordbookCenterPage /></Suspense></AdminProtectedRoute>)} />
          </Route>
          <Route path="*" component={() => (<Suspense fallback={<PageSpinner />}><NotFoundPage /></Suspense>)} />
        </Route>
        <Route path="*" component={() => (<Suspense fallback={<PageSpinner />}><NotFoundPage /></Suspense>)} />
      </Router>
      </MaintenanceProvider>
      <Toaster />
    </AppErrorBoundary>
  );
}
