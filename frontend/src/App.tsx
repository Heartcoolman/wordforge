import { Router, Route } from '@solidjs/router';
import { lazy, Suspense, Show, createSignal, onMount, onCleanup } from 'solid-js';
import { Toaster } from '@/components/ui/Toast';
import { AppErrorBoundary } from '@/components/ErrorBoundary';
import { PageLayout } from '@/components/layout/PageLayout';
import { AdminLayout } from '@/components/layout/AdminLayout';
import { ProtectedRoute } from '@/components/auth/ProtectedRoute';
import { AdminProtectedRoute } from '@/components/auth/ProtectedRoute';
import { Spinner } from '@/components/ui/Spinner';
import { api, maintenanceActive, setMaintenanceActive, setUpdateInfo, connectSseStream } from '@/api/client';
import { startTelemetryWorker, stopTelemetryWorker, handleTelemetryRequest } from '@/workers/telemetry';
import MaintenancePage from '@/pages/MaintenancePage';
import { UpdateBanner } from '@/components/ui/UpdateBanner';

const NotFoundPage = lazy(() => import('@/pages/NotFoundPage'));

// Lazy-loaded pages
const HomePage = lazy(() => import('@/pages/HomePage'));
const LoginPage = lazy(() => import('@/pages/LoginPage'));
const RegisterPage = lazy(() => import('@/pages/RegisterPage'));
const LearningPage = lazy(() => import('@/pages/LearningPage'));
const FlashcardPage = lazy(() => import('@/pages/FlashcardPage'));
const VocabularyPage = lazy(() => import('@/pages/VocabularyPage'));
const WordbookPage = lazy(() => import('@/pages/WordbookPage'));
const WordbookCenterPage = lazy(() => import('@/pages/WordbookCenterPage'));
const StatisticsPage = lazy(() => import('@/pages/StatisticsPage'));
const HistoryPage = lazy(() => import('@/pages/HistoryPage'));
const ProfilePage = lazy(() => import('@/pages/ProfilePage'));
const NotificationsPage = lazy(() => import('@/pages/NotificationsPage'));

// Admin pages
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
      <MaintenanceProvider>
      <Router>
        <Route path="/" component={PageLayout}>
          <Route
            path="/"
            component={() => (
              <Suspense fallback={<PageSpinner />}>
                <HomePage />
              </Suspense>
            )}
          />
          <Route
            path="/login"
            component={() => (
              <Suspense fallback={<PageSpinner />}>
                <LoginPage />
              </Suspense>
            )}
          />
          <Route
            path="/register"
            component={() => (
              <Suspense fallback={<PageSpinner />}>
                <RegisterPage />
              </Suspense>
            )}
          />
          <Route
            path="/learning"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <LearningPage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="/flashcard"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <FlashcardPage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="/vocabulary"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <VocabularyPage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="/wordbooks"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <WordbookPage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="/wordbook-center"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <WordbookCenterPage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="/statistics"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <StatisticsPage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="/history"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <HistoryPage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="/profile"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <ProfilePage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="/notifications"
            component={() => (
              <ProtectedRoute>
                <Suspense fallback={<PageSpinner />}>
                  <NotificationsPage />
                </Suspense>
              </ProtectedRoute>
            )}
          />
          <Route
            path="*"
            component={() => (
              <Suspense fallback={<PageSpinner />}>
                <NotFoundPage />
              </Suspense>
            )}
          />
        </Route>

        {/* Admin routes */}
        <Route path="/admin">
          <Route
            path="/login"
            component={() => (
              <Suspense fallback={<PageSpinner />}>
                <AdminLoginPage />
              </Suspense>
            )}
          />
          <Route
            path="/setup"
            component={() => (
              <Suspense fallback={<PageSpinner />}>
                <AdminSetupPage />
              </Suspense>
            )}
          />
          <Route path="/" component={AdminLayout}>
            <Route
              path="/"
              component={() => (
                <AdminProtectedRoute>
                  <Suspense fallback={<PageSpinner />}>
                    <AdminDashboard />
                  </Suspense>
                </AdminProtectedRoute>
              )}
            />
            <Route
              path="/users"
              component={() => (
                <AdminProtectedRoute>
                  <Suspense fallback={<PageSpinner />}>
                    <UserManagementPage />
                  </Suspense>
                </AdminProtectedRoute>
              )}
            />
            <Route
              path="/clients"
              component={() => (
                <AdminProtectedRoute>
                  <Suspense fallback={<PageSpinner />}>
                    <ClientsPage />
                  </Suspense>
                </AdminProtectedRoute>
              )}
            />
            <Route
              path="/amas-config"
              component={() => (
                <AdminProtectedRoute>
                  <Suspense fallback={<PageSpinner />}>
                    <AmasConfigPage />
                  </Suspense>
                </AdminProtectedRoute>
              )}
            />
            <Route
              path="/monitoring"
              component={() => (
                <AdminProtectedRoute>
                  <Suspense fallback={<PageSpinner />}>
                    <MonitoringPage />
                  </Suspense>
                </AdminProtectedRoute>
              )}
            />
            <Route
              path="/analytics"
              component={() => (
                <AdminProtectedRoute>
                  <Suspense fallback={<PageSpinner />}>
                    <AnalyticsPage />
                  </Suspense>
                </AdminProtectedRoute>
              )}
            />
            <Route
              path="/settings"
              component={() => (
                <AdminProtectedRoute>
                  <Suspense fallback={<PageSpinner />}>
                    <SettingsPage />
                  </Suspense>
                </AdminProtectedRoute>
              )}
            />
            <Route
              path="/wordbook-center"
              component={() => (
                <AdminProtectedRoute>
                  <Suspense fallback={<PageSpinner />}>
                    <AdminWordbookCenterPage />
                  </Suspense>
                </AdminProtectedRoute>
              )}
            />
          </Route>
          <Route
            path="*"
            component={() => (
              <Suspense fallback={<PageSpinner />}>
                <NotFoundPage />
              </Suspense>
            )}
          />
        </Route>
      </Router>
      </MaintenanceProvider>
      <Toaster />
    </AppErrorBoundary>
  );
}
