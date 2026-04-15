## Why

当前 wordforge 仓库是一个单体架构：Rust 后端 + 用户前端 + 管理员前端全部耦合在同一仓库中。前端代码位于 `frontend/` 目录，通过 Vite 构建到 `static/` 目录，由后端 `tower-http::ServeDir` 提供 SPA 服务。用户页面（12 个）和管理员页面（10 个）共享同一个 `App.tsx` 路由树、同一套 UI 组件库、同一个 `tokenManager`（同时管理用户 token 和管理员 token）。

这种架构导致三个问题：

1. **部署耦合**：用户前端的任何变更都需要重新构建整个前端并重新部署后端，管理员前端同理。
2. **职责混淆**：用户代码和管理员代码混在一起，`tokenManager` 同时处理两种身份的 token，`ProtectedRoute` 和 `AdminProtectedRoute` 定义在同一个文件中。
3. **无法独立扩展**：用户前端无法独立部署到 CDN 或独立域名，无法针对用户端做独立的性能优化和缓存策略。

本次变更将用户前端剥离为独立仓库，实现独立构建和跨域独立部署。现有仓库保留后端 + 管理员前端，成为真正的服务端仓库。管理员前端的所有现有功能保持不变。

## What Changes

- 创建独立的用户前端仓库，包含所有用户页面、用户 API 模块、用户 stores、用户 hooks、用户 workers，以及从现有仓库直接复制的共享基础设施代码（UI 组件、工具函数、类型定义）
- 用户前端仓库的 `tokenManager` 仅保留用户 token 逻辑（`getToken`/`setTokens`/`clearTokens`/`refreshAccessToken`），移除所有管理员 token 方法
- 用户前端仓库的 `api/client.ts` 移除 `useAdminToken` 选项，`resolveApiBase()` 强制要求 `VITE_API_BASE_URL` 环境变量指向后端 API 地址
- 现有仓库 `frontend/` 目录中的管理员前端保持不变——所有管理员页面、管理员路由、管理员 API 模块、管理员 stores 均不做任何修改
- 后端 CORS 配置（`src/main.rs:162-242` 的 `build_cors_layer`）已支持多 origin（逗号分隔），部署时将 `CORS_ORIGIN` 环境变量设置为包含用户前端域名
- 后端 Cookie 策略变更（`src/routes/auth.rs:429-453`）：将 `token` 和 `refresh_token` cookie 的 `SameSite=Strict` 改为 `SameSite=None`，以支持跨站部署。需要修改 `set_token_cookie`、`set_refresh_token_cookie`、`clear_auth_cookies` 三个函数
- 后端 SPA 路由限制：修改 `tower-http::ServeDir` 的 fallback 逻辑，仅对 `/admin/*` 路径提供 SPA fallback，其他非 API 路径返回 404 或重定向到用户前端域名

## Capabilities

### New Capabilities

- `user-frontend-repo`: 独立的用户前端仓库，包含完整的 Solid.js + Vite 构建配置、所有用户页面（`HomePage`/`LoginPage`/`RegisterPage`/`LearningPage`/`FlashcardPage`/`VocabularyPage`/`WordbookPage`/`WordbookCenterPage`/`StatisticsPage`/`HistoryPage`/`ProfilePage`/`NotificationsPage`）、用户专用 API 模块、独立的 `tokenManager`（仅用户 token）、独立的 `ProtectedRoute`（仅用户路由守卫）、以及从现有仓库复制的共享代码（`components/ui/*`、`utils/*`、`lib/*`、`types/*`）

### Modified Capabilities

- `backend-cookie-policy`: 后端 Cookie 策略从 `SameSite=Strict` 改为 `SameSite=None`，支持跨站用户前端部署
- `backend-spa-routing`: 后端 SPA fallback 限制为仅 `/admin/*` 路径，阻止旧用户前端继续对外服务

## Impact

**新仓库（用户前端）**：

需要创建的目录结构：
```
wordforge-web/
├── src/
│   ├── api/              ← 从 frontend/src/api/ 复制：client.ts（移除 useAdminToken）、auth.ts、learning.ts、records.ts、wordbooks.ts、words.ts、wordStates.ts、studyConfig.ts、notifications.ts、userProfile.ts、wordbookCenter.ts、content.ts、users.ts、amas.ts、health.ts
│   ├── components/
│   │   ├── ui/           ← 从 frontend/src/components/ui/ 完整复制
│   │   ├── layout/       ← 复制 PageLayout.tsx、Navigation.tsx
│   │   ├── auth/         ← 复制 ProtectedRoute.tsx（仅保留 ProtectedRoute，移除 AdminProtectedRoute）
│   │   ├── fatigue/      ← 从 frontend/src/components/fatigue/ 完整复制
│   │   ├── ErrorBoundary.tsx
│   │   └── SystemLockedModal.tsx
│   ├── hooks/            ← 从 frontend/src/hooks/ 复制 useFatigueDetection.ts
│   ├── lib/              ← 从 frontend/src/lib/ 复制全部，token.ts 需重构（移除管理员 token 方法）
│   ├── pages/            ← 从 frontend/src/pages/ 复制所有用户页面（12 个），不复制 admin/ 子目录
│   ├── stores/           ← 从 frontend/src/stores/ 复制 auth.ts（移除管理员 token 引用）、theme.ts、ui.ts、learning.ts、fatigue.ts
│   ├── types/            ← 从 frontend/src/types/ 复制全部用户类型 + 共享类型（api.ts、user.ts、amas.ts），不复制 admin.ts
│   ├── utils/            ← 从 frontend/src/utils/ 完整复制
│   ├── workers/          ← 从 frontend/src/workers/ 复制 fatigue.worker.ts、telemetry.ts
│   ├── App.tsx           ← 新建，仅包含用户路由（/ 下的 12 个页面 + MaintenanceProvider + SystemLockedModal）
│   ├── index.css         ← 从 frontend/src/index.css 复制
│   ├── main.tsx          ← 从 frontend/src/main.tsx 复制
│   └── vite-env.d.ts     ← 从 frontend/src/vite-env.d.ts 复制
├── public/               ← 从 frontend/public/ 复制
├── index.html            ← 从 frontend/index.html 复制
├── package.json          ← 基于 frontend/package.json，移除不需要的依赖
├── tsconfig.json         ← 从 frontend/tsconfig.json 复制
├── vite.config.ts        ← 基于 frontend/vite.config.ts，修改 outDir 和 proxy 配置
└── .gitignore            ← 从 frontend/.gitignore 复制
```

需要重构的文件：
- `src/lib/token.ts`：移除 `inMemoryAdminToken` 变量、`getAdminToken()`、`setAdminToken()`、`clearAdminToken()`、`isAdminTokenExpiringSoon()` 方法
- `src/api/client.ts`：移除 `ReqOpts` 中的 `useAdminToken` 选项及相关逻辑；`resolveApiBase()` 在 `VITE_API_BASE_URL` 未设置时抛出错误而非回退到 `window.location.origin`
- `src/api/amas.ts`：拆分为 `amas-user.ts`（保留 `getVisualFatigueMetrics`、`connectAmasStream` 等用户端点）和 `amas-admin.ts`（`getAmasConfig`、`updateAmasConfig`、`getAmasMetrics`、`getMonitoringEvents`）。新仓库仅复制 `amas-user.ts`
- `src/components/auth/ProtectedRoute.tsx`：仅保留 `ProtectedRoute` 导出，移除 `AdminProtectedRoute`
- `src/stores/auth.ts`：移除管理员 token 相关的 signal 和方法
- `src/App.tsx`：仅包含用户路由树（`/` 下的 12 个页面路由 + `*` 404），移除所有 `/admin` 路由和 `AdminLayout` 引用。移除 `MaintenanceProvider` 中的 `isAdminPath()` 检查
- `vite.config.ts`：`outDir` 改为 `dist`；`server.proxy` 的 target 从 `VITE_DEV_API_TARGET` 环境变量读取（默认 `http://localhost:3000`）；`@fatigue-wasm` 别名改为指向 `node_modules/@wordforge/visual-fatigue-wasm`；构建时校验 `VITE_API_BASE_URL` 必填

**现有仓库（后端 + 管理员前端）**：

需要修改的文件：
- `src/routes/auth.rs:429-453`：将 `set_token_cookie`、`set_refresh_token_cookie`、`clear_auth_cookies` 中的 `SameSite=Strict` 改为 `SameSite=None`
- `src/main.rs`：修改 `tower-http::ServeDir` fallback 逻辑，仅对 `/admin/*` 路径提供 SPA fallback，其他非 `/api` 路径返回 404

部署层面的变更：
- `CORS_ORIGIN` 环境变量需要设置为逗号分隔的多 origin 值。例如：`CORS_ORIGIN=https://wordforge.com,https://wordforge-app.com`。后端 `build_cors_layer`（`src/main.rs:162-242`）已原生支持此格式

## 约束与注意事项

1. **共享代码采用直接复制策略**：UI 组件（`components/ui/*`）、工具函数（`utils/*`）、类型定义（`types/*`）直接复制到用户前端仓库。两个仓库独立维护各自的副本，后续不保证同步。
2. **Token 管理完全解耦**：用户前端仓库的 `tokenManager` 仅处理用户 access token 和 refresh token（HttpOnly cookie）；管理员前端继续使用现有的完整 `tokenManager`（包含管理员 token 逻辑）。
3. **跨站部署与 Cookie 策略**：用户前端部署到 `wordforge-app.com`，后端 + 管理前端部署到 `wordforge.com`。后端 Cookie 改为 `SameSite=None; Secure`，CORS 配置 `allow_credentials: true`。已知风险：Chrome 正逐步限制第三方 cookie，`SameSite=None` 在未来可能被浏览器拦截。降级方案：迁移到同顶级域子域部署（`app.wordforge.com` / `api.wordforge.com`），将 Cookie 改为 `SameSite=Lax; Domain=.wordforge.com`。
4. **SSE 实现固定为 fetch + ReadableStream**：当前代码使用 `fetch` + `getReader()` 流式读取（非原生 `EventSource`），跨域时通过 `credentials: 'include'` 携带 cookie。禁止改为原生 `EventSource`（不支持自定义 Authorization header）。
5. **后端路由限制**：修改后端 SPA fallback，仅对 `/admin/*` 路径提供 `index.html` 回退。用户访问旧域名的非 API、非 admin 路径返回 404。通过此机制规避 refresh token 竞争条件（旧用户前端不再可用）。
6. **API 模块裁剪**：`amas.ts` 拆分为 `amas-user.ts`（用户端点）和 `amas-admin.ts`（管理端点），新仓库仅复制 `amas-user.ts`。`health.ts` 同理，裁剪管理端函数。
7. **WASM 依赖供应**：`visual-fatigue-wasm` 发布为私有 npm 包，新仓库通过 `npm install` 引入。`vite.config.ts` 中的 `@fatigue-wasm` 别名改为指向 `node_modules` 中的包路径。
8. **MediaPipe 资源**：允许从外部 CDN 加载 MediaPipe WASM/模型文件。CSP `connect-src` 和 `worker-src` 白名单需包含 MediaPipe CDN 域名。
9. **环境变量契约**：
   - `VITE_API_BASE_URL`（必填）：后端 API 地址。构建时缺失则 `vite build` 报错退出。
   - `VITE_APP_ENV`（可选）：`development` / `staging` / `production`，默认 `production`。
   - dev proxy 环境变量：`VITE_DEV_API_TARGET`，默认 `http://localhost:3000`。
10. **SPA Fallback**：新仓库部署使用 Nginx，配置 `try_files $uri /index.html` 处理前端路由。
11. **JSX 编译链**：新仓库必须使用 `vite-plugin-solid`，`tsconfig.json` 设置 `jsx: preserve`、`jsxImportSource: solid-js`。禁止使用 React JSX 编译配置。
12. **CI/CD 流程**：新仓库配置完整 GitHub Actions：lint → type-check → unit test (Vitest) → build → E2E test (Playwright) → deploy。构建失败阻断 merge。
13. **迁移审计要求**：`Navigation.tsx`、`PageLayout.tsx`、`App.tsx` 必须通过去-admin 审计，确保无 `/admin` 路径引用、无 `AdminLayout` 引用、无 `useAdminToken` 调用。
