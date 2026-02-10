import { Router, Route } from '@solidjs/router';
import { lazy, Suspense } from 'solid-js';
import { Toaster } from '@/components/ui/Toast';
import { AppErrorBoundary } from '@/components/ErrorBoundary';
import { PageLayout } from '@/components/layout/PageLayout';
import { AdminLayout } from '@/components/layout/AdminLayout';
import { ProtectedRoute } from '@/components/auth/ProtectedRoute';
import { AdminProtectedRoute } from '@/components/auth/ProtectedRoute';
import { Spinner } from '@/components/ui/Spinner';

const NotFoundPage = lazy(() => import('@/pages/NotFoundPage'));

// Lazy-loaded pages
const HomePage = lazy(() => import('@/pages/HomePage'));
const LoginPage = lazy(() => import('@/pages/LoginPage'));
const RegisterPage = lazy(() => import('@/pages/RegisterPage'));
const LearningPage = lazy(() => import('@/pages/LearningPage'));
const FlashcardPage = lazy(() => import('@/pages/FlashcardPage'));
const VocabularyPage = lazy(() => import('@/pages/VocabularyPage'));
const WordbookPage = lazy(() => import('@/pages/WordbookPage'));
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

function PageSpinner() {
  return (
    <div class="flex items-center justify-center min-h-[60vh]">
      <Spinner size="lg" />
    </div>
  );
}

export default function App() {
  return (
    <AppErrorBoundary>
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
      <Toaster />
    </AppErrorBoundary>
  );
}
