## MODIFIED Requirements

### Requirement: Cookie SameSite policy changed to None
The backend `src/routes/auth.rs` functions `set_token_cookie`, `set_refresh_token_cookie`, and `clear_auth_cookies` MUST change Cookie `SameSite` from `Strict` to `None` to support cross-site user frontend deployment.

#### Scenario: Token cookie set with SameSite=None
- **WHEN** user logs in successfully
- **THEN** `token` cookie is set with `SameSite=None; Secure; HttpOnly; Path=/`

#### Scenario: Refresh token cookie set with SameSite=None
- **WHEN** user logs in successfully
- **THEN** `refresh_token` cookie is set with `SameSite=None; Secure; HttpOnly; Path=/`

#### Scenario: Cookie clearing uses SameSite=None
- **WHEN** user logs out
- **THEN** both `token` and `refresh_token` cookies are cleared with `SameSite=None; Secure; HttpOnly; Path=/; Max-Age=0`

#### Scenario: Cross-origin requests carry cookies
- **WHEN** user frontend at `wordforge-app.com` sends request to backend at `wordforge.com` with `credentials: include`
- **THEN** browser includes `token` and `refresh_token` cookies in the request

#### Scenario: Third-party cookie degradation path documented
- **WHEN** browsers block third-party cookies with `SameSite=None`
- **THEN** degradation plan is to migrate to subdomain deployment (`app.wordforge.com` / `api.wordforge.com`) with `SameSite=Lax; Domain=.wordforge.com`

### Requirement: Cookie PBT invariants
The cookie system MUST satisfy the following property-based testing invariants.

#### Scenario: SameSite=None always implies Secure flag
- **WHEN** any auth cookie header is produced by `set_token_cookie`, `set_refresh_token_cookie`, or `clear_auth_cookies`
- **THEN** if `SameSite=None` is present, `Secure` flag MUST also be present
- **THEN** no cookie operation produces `SameSite=None` without `Secure`

#### Scenario: Cookie policy completeness across all operations
- **WHEN** parsing all auth cookie headers from set and clear operations
- **THEN** the policy vector `(Path=/, HttpOnly=true, Secure=true, SameSite=None)` is identical across all operations
- **THEN** only `name`, `value`, and `Max-Age` may differ between set and clear operations

#### Scenario: Cookie round-trip integrity
- **WHEN** `set_token_cookie(response, token)` is called with any valid JWT-format token
- **THEN** parsing the resulting `Set-Cookie` header yields the original token value unchanged

#### Scenario: Cookie idempotency
- **WHEN** `set_token_cookie` is called N times (N >= 1) with the same token value
- **THEN** the final browser-visible cookie state is identical to calling it once
