## ADDED Requirements

### Requirement: Standalone user frontend repository
The system MUST provide a standalone user frontend repository `wordforge-web` containing all user pages, user API modules, user stores, user hooks, user workers, and shared infrastructure code copied from the existing repository. It MUST use Solid.js + Vite for building and SHALL be deployed to `wordforge-app.com`.

#### Scenario: Project builds successfully with required env vars
- **WHEN** `VITE_API_BASE_URL` is set and `npm run build` is executed
- **THEN** Vite produces a `dist/` directory with valid SPA assets

#### Scenario: Build fails when VITE_API_BASE_URL is missing
- **WHEN** `VITE_API_BASE_URL` is not set and `npm run build` is executed
- **THEN** `vite build` exits with a non-zero code and an error message indicating the missing variable

#### Scenario: Dev server proxies API requests
- **WHEN** `npm run dev` is executed with `VITE_DEV_API_TARGET=http://localhost:3000`
- **THEN** requests to `/api/*` are proxied to the target backend

### Requirement: Token management decoupled from admin
The user frontend repository's `tokenManager` MUST only handle user access token and refresh token. It MUST NOT contain any admin token logic.

#### Scenario: tokenManager exports only user token methods
- **WHEN** `src/lib/token.ts` is imported
- **THEN** only `getToken`, `setTokens`, `clearTokens`, `refreshAccessToken`, `isTokenExpiringSoon` are exported
- **THEN** `getAdminToken`, `setAdminToken`, `clearAdminToken`, `isAdminTokenExpiringSoon` do not exist

#### Scenario: API client has no admin token support
- **WHEN** `src/api/client.ts` is imported
- **THEN** `ReqOpts` type does not contain `useAdminToken` field
- **THEN** `resolveApiBase()` returns `VITE_API_BASE_URL` value without fallback to `window.location.origin`

### Requirement: API module separation from admin endpoints
The user frontend repository's API modules MUST NOT contain any admin endpoints. `amas.ts` MUST be split into `amas-user.ts` (user endpoints) and `amas-admin.ts` (admin endpoints). The new repository SHALL only include `amas-user.ts`.

#### Scenario: amas-user.ts contains only user endpoints
- **WHEN** `src/api/amas-user.ts` is imported
- **THEN** no function calls `/api/admin/*` endpoints
- **THEN** no function uses `useAdminToken: true` option

#### Scenario: No admin API references in user frontend
- **WHEN** searching all files in `src/api/` for `/api/admin/` or `useAdminToken`
- **THEN** zero matches are found

### Requirement: WASM dependency via npm package
The fatigue detection WASM dependency MUST be supplied via a private npm package `@wordforge/visual-fatigue-wasm`. It MUST NOT depend on monorepo-internal local paths.

#### Scenario: Vite resolves WASM alias from node_modules
- **WHEN** `@fatigue-wasm` alias is used in imports
- **THEN** Vite resolves it to `node_modules/@wordforge/visual-fatigue-wasm`

### Requirement: SSE uses fetch + ReadableStream only
SSE connections MUST use `fetch` + `getReader()` streaming. Cross-origin requests MUST include `credentials: 'include'` to carry cookies. The system MUST NOT use native `EventSource`.

#### Scenario: SSE connection works cross-origin
- **WHEN** user frontend at `wordforge-app.com` connects to SSE endpoint at `wordforge.com`
- **THEN** fetch request includes `credentials: 'include'`
- **THEN** response is read via `ReadableStream.getReader()`

### Requirement: Migration audit for admin references
All files migrated to the new repository MUST pass an admin-reference audit. No admin path references, admin layout imports, or admin token calls SHALL remain.

#### Scenario: No admin references in migrated files
- **WHEN** searching `Navigation.tsx`, `PageLayout.tsx`, `App.tsx` for admin-related references
- **THEN** no `/admin` path references, no `AdminLayout` imports, no `useAdminToken` calls exist

### Requirement: JSX compilation chain locked to Solid.js
The new repository MUST use `vite-plugin-solid` for JSX compilation. React JSX compilation configuration MUST NOT be used.

#### Scenario: TSX files compile as Solid.js components
- **WHEN** `vite.config.ts` is inspected
- **THEN** `vite-plugin-solid` is configured as a plugin
- **WHEN** `tsconfig.json` is inspected
- **THEN** `jsx` is set to `preserve` and `jsxImportSource` is set to `solid-js`

### Requirement: Complete CI/CD pipeline
The new repository MUST have a complete GitHub Actions CI/CD pipeline. Build failures MUST block merge.

#### Scenario: CI pipeline runs on pull request
- **WHEN** a pull request is opened
- **THEN** pipeline executes: lint → type-check → unit test (Vitest) → build → E2E test (Playwright)
- **THEN** any step failure blocks merge

#### Scenario: Deploy pipeline runs on main merge
- **WHEN** code is merged to main branch
- **THEN** build artifacts are deployed to production hosting

### Requirement: SPA fallback via Nginx
The new repository deployment MUST use Nginx with SPA fallback configuration. All non-asset requests MUST be served `index.html`.

#### Scenario: Direct URL access works for all user routes
- **WHEN** user navigates directly to `/learning`, `/profile`, or any user route
- **THEN** Nginx serves `index.html` via `try_files $uri /index.html`

### Requirement: MediaPipe resources from external CDN
The fatigue detection worker MUST load MediaPipe WASM/model files from external CDN. CSP headers MUST whitelist MediaPipe CDN domains.

#### Scenario: MediaPipe resources load successfully
- **WHEN** fatigue detection worker initializes
- **THEN** MediaPipe WASM and model files are fetched from external CDN
- **THEN** CSP headers allow connections to MediaPipe CDN domains
