import { Router, Route, useLocation } from '@solidjs/router';
import { lazy, Suspense, Show, onMount, onCleanup } from 'solid-js';
import { Toaster } from '@/components/ui/Toast';
import { AppErrorBoundary } from '@/components/ErrorBoundary';
import { AdminLayout } from '@/components/layout/AdminLayout';
import { AdminProtectedRoute } from '@/components/auth/ProtectedRoute';
import { Spinner } from '@/components/ui/Spinner';
import { api, maintenanceActive, setMaintenanceActive, setUpdateInfo } from '@/api/client';
import MaintenancePage from '@/pages/MaintenancePage';
import { UpdateBanner } from '@/components/ui/UpdateBanner';

const NotFoundPage = lazy(() => import('@/pages/NotFoundPage'));
const LegacyUserFrontendPage = lazy(() => import('@/pages/LegacyUserFrontendPage'));

const AdminLoginPage = lazy(() => import('@/pages/admin/AdminLoginPage'));
const AdminSetupPage = lazy(() => import('@/pages/admin/AdminSetupPage'));
const AdminDashboard = lazy(() => import('@/pages/admin/AdminDashboard'));
const UserManagementPage = lazy(() => import('@/pages/admin/UserManagementPage'));
const AmasConfigPage = lazy(() => import('@/pages/admin/AmasConfigPage'));
const AmasMetricsPage = lazy(() => import('@/pages/admin/AmasMetricsPage'));
const AmasAdvisorPage = lazy(() => import('@/pages/admin/AmasAdvisorPage'));
const MonitoringPage = lazy(() => import('@/pages/admin/MonitoringPage'));
const AnalyticsPage = lazy(() => import('@/pages/admin/AnalyticsPage'));
const SettingsPage = lazy(() => import('@/pages/admin/SettingsPage'));
const UpdatesPage = lazy(() => import('@/pages/admin/UpdatesPage'));
const AdminWordbookCenterPage = lazy(() => import('@/pages/admin/AdminWordbookCenterPage'));
const ClientsPage = lazy(() => import('@/pages/admin/ClientsPage'));

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
  });

  onCleanup(() => {
    if (pollTimer) clearInterval(pollTimer);
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
          <Route path="/login" component={() => (<Suspense fallback={<PageSpinner />}><AdminLoginPage /></Suspense>)} />
          <Route path="/setup" component={() => (<Suspense fallback={<PageSpinner />}><AdminSetupPage /></Suspense>)} />
          <Route path="/" component={AdminLayout}>
            <Route path="/" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AdminDashboard /></Suspense></AdminProtectedRoute>)} />
            <Route path="/users" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><UserManagementPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/clients" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><ClientsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/amas-config" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AmasConfigPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/amas-metrics" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AmasMetricsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/amas-advisor" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AmasAdvisorPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/monitoring" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><MonitoringPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/analytics" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AnalyticsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/settings" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><SettingsPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/updates" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><UpdatesPage /></Suspense></AdminProtectedRoute>)} />
            <Route path="/wordbook-center" component={() => (<AdminProtectedRoute><Suspense fallback={<PageSpinner />}><AdminWordbookCenterPage /></Suspense></AdminProtectedRoute>)} />
          </Route>
          <Route path="*" component={() => (<Suspense fallback={<PageSpinner />}><NotFoundPage /></Suspense>)} />
        </Route>
        <Route path="*" component={() => (<Suspense fallback={<PageSpinner />}><NotFoundPage /></Suspense>)} />
      </Router>
      <Toaster />
    </AppErrorBoundary>
  );
}
