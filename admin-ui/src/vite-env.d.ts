/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_USER_APP_URL?: string;
  // Sentry：DSN 缺省时 initSentry 会跳过初始化，本地开发不依赖外部服务
  readonly VITE_SENTRY_DSN?: string;
  readonly VITE_SENTRY_ENV?: string;
  readonly VITE_SENTRY_RELEASE?: string;
  readonly VITE_SENTRY_TRACES_SAMPLE_RATE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
