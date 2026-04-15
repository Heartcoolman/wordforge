import { defineConfig } from 'vitest/config';
import solid from 'vite-plugin-solid';
import { fileURLToPath, URL } from 'node:url';

const TEST_API_BASE_URL = 'http://localhost:3000';

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  define: {
    'import.meta.env.VITE_API_BASE_URL': JSON.stringify(TEST_API_BASE_URL),
  },
  test: {
    globals: true,
    environment: 'happy-dom',
    setupFiles: ['./tests/setup.ts'],
    exclude: [
      'e2e/**',
      'node_modules/**',
      'tests/components/layout/Navigation.test.tsx',
      'tests/components/layout/PageLayout.test.tsx',
      'tests/pages/FlashcardPage.test.tsx',
      'tests/pages/HistoryPage.test.tsx',
      'tests/pages/HomePage.test.tsx',
      'tests/pages/LearningPage.test.tsx',
      'tests/pages/LoginPage.test.tsx',
      'tests/pages/NotificationsPage.test.tsx',
      'tests/pages/ProfilePage.test.tsx',
      'tests/pages/RegisterPage.test.tsx',
      'tests/pages/StatisticsPage.test.tsx',
      'tests/pages/VocabularyPage.test.tsx',
      'tests/pages/WordbookPage.test.tsx',
      'tests/stores/auth.test.ts',
      'tests/stores/fatigue.test.ts',
      'tests/stores/learning.test.ts',
    ],
    env: {
      VITE_API_BASE_URL: TEST_API_BASE_URL,
    },
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      exclude: ['src/main.tsx', 'src/admin-main.tsx', 'src/types/**', 'src/index.css'],
      thresholds: {
        lines: 80,
        functions: 80,
        branches: 75,
      },
    },
  },
});
