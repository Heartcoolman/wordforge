## MODIFIED Requirements

### Requirement: SPA fallback restricted to admin routes only
The backend `tower-http::ServeDir` fallback logic MUST be modified to only provide SPA `index.html` fallback for `/admin/*` paths. Other non-`/api` paths MUST return 404.

#### Scenario: Admin routes get SPA fallback
- **WHEN** browser requests `/admin/dashboard` or any `/admin/*` path
- **THEN** backend serves `static/index.html` as SPA fallback

#### Scenario: User routes no longer served by backend
- **WHEN** browser requests `/learning`, `/profile`, or any non-admin, non-API path
- **THEN** backend returns HTTP 404

#### Scenario: API routes unaffected
- **WHEN** browser requests `/api/*` endpoints
- **THEN** backend routes to API handlers as before

#### Scenario: Static assets still served
- **WHEN** browser requests static assets (JS, CSS, images) from `/static/*`
- **THEN** backend serves them from the `static/` directory

### Requirement: SPA routing PBT invariants
The SPA routing system MUST satisfy the following property-based testing invariants.

#### Scenario: Admin prefix boundary is strict
- **WHEN** request path matches regex `^/admin(/.*)?$`
- **THEN** backend serves `index.html` as SPA fallback
- **WHEN** request path is `/admins`, `/admin-panel`, `/administrator`, `/Admin`, or `/%61dmin`
- **THEN** backend returns 404, NOT `index.html`

#### Scenario: Route partition is exclusive and exhaustive
- **WHEN** any request path is received
- **THEN** it maps to exactly one category: API route, Admin SPA fallback, Static asset, or 404
- **THEN** no path maps to more than one category

#### Scenario: Non-admin 404 monotonicity
- **WHEN** a non-admin path returns 404
- **THEN** adding new `/admin/*` sub-routes does not change the 404 status of that path
