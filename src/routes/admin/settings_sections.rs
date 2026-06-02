//! settings.html 本地生效面板的强类型配置体。
//!
//! PUT /api/admin/settings/config/:section 对以下 5 个 section 做强类型 serde
//! 解析 + 字段校验,通过后归一化(normalize)再落 settings_config 存储:
//!   - site          站点与外观
//!   - auth          认证与登录
//!   - ratelimit     速率限制
//!   - audit-config  审计日志配置
//!   - backup-policy 备份策略
//!
//! 其余 section(roles/email/sms/storage/db/keys 等)沿用原有"裸 JSON object"
//! 透传路径,不在此校验。
//!
//! 设计约束(见任务说明):
//!   - 外部服务凭据(SMTP/SMS/对象存储/DB/缓存)只存配置 + 提供"格式校验"测试桩,
//!     不在此集成真实外部服务。
//!   - ratelimit:运行期中间件从启动期 Config 读取配额,非本存储;此处仅持久化 +
//!     文档化(运维需重启或后续接通),不做 live 接线(非 trivial)。
//!   - backup-policy:只持久化;备份执行已在 /admin/updates 下存在。

use serde::{Deserialize, Serialize};

use crate::response::AppError;

/// 已强类型化的 section 白名单。命中则走 [`parse_typed_section`];否则按裸 JSON 透传。
pub const TYPED_SECTIONS: &[&str] = &["site", "auth", "ratelimit", "audit-config", "backup-policy"];

/// 各 section 的敏感字段路径(顶层 key 或 "outer.inner" 一层嵌套)。GET 时按此遮蔽,
/// PUT 时若传入空/遮蔽串则保留旧值。当前无 section 含密钥字段。
pub const SECRET_FIELDS: &[(&str, &[&str])] = &[];

/// GET 时回显的遮蔽串。PUT 时若传入此值表示"不修改密钥"。
pub const SECRET_MASK: &str = "••••••";

fn invalid(field: &str) -> AppError {
    AppError::bad_request("INVALID_SECTION_FIELD", field)
}

/// 取某 section 的敏感字段路径列表(无则空)。
fn secret_paths(section: &str) -> &'static [&'static str] {
    SECRET_FIELDS
        .iter()
        .find(|(s, _)| *s == section)
        .map(|(_, f)| *f)
        .unwrap_or(&[])
}

/// 按路径取 JSON 对象内的可变引用(支持一层 "a.b" 嵌套)。不存在返回 None。
fn path_get_mut<'a>(
    obj: &'a mut serde_json::Value,
    path: &str,
) -> Option<&'a mut serde_json::Value> {
    let mut cur = obj;
    for seg in path.split('.') {
        cur = cur.as_object_mut()?.get_mut(seg)?;
    }
    Some(cur)
}

/// 返回 section 配置的"GET 遮蔽视图":把全部敏感字段值替换为遮蔽串(若该字段存在且为非空字符串)。
/// 非密钥 section 原样返回。
pub fn mask_secrets(section: &str, json: &serde_json::Value) -> serde_json::Value {
    let paths = secret_paths(section);
    if paths.is_empty() {
        return json.clone();
    }
    let mut out = json.clone();
    for p in paths {
        if let Some(v) = path_get_mut(&mut out, p) {
            if v.as_str().map(|s| !s.is_empty()).unwrap_or(false) {
                *v = serde_json::Value::String(SECRET_MASK.to_string());
            }
        }
    }
    out
}

/// 密钥保留合并:PUT 解析后的归一化体里,凡敏感字段为空串或遮蔽串者,用 existing 的旧值回填。
/// 这样前端回显遮蔽串、原样提交时不会把真实密钥冲掉。existing 为已存储的原始(未遮蔽)体。
pub fn preserve_secrets(
    section: &str,
    normalized: &mut serde_json::Value,
    existing: Option<&serde_json::Value>,
) {
    for p in secret_paths(section) {
        let incoming_blank = path_get_mut(normalized, p)
            .and_then(|v| v.as_str())
            .map(|s| s.is_empty() || s == SECRET_MASK)
            .unwrap_or(true);
        if !incoming_blank {
            continue;
        }
        let old = existing
            .cloned()
            .as_mut()
            .and_then(|e| path_get_mut(e, p).cloned());
        if let (Some(slot), Some(old_val)) = (path_get_mut(normalized, p), old) {
            *slot = old_val;
        }
    }
}

/// 解析并校验一个强类型 section。成功返回归一化后的 JSON object。
///
/// 仅当 `section ∈ TYPED_SECTIONS` 时调用。返回的 `(json, registration_open)`:
/// 第二项仅 `auth` section 非 None,用于向后兼容同步 system_settings.registration_enabled。
pub fn parse_typed_section(
    section: &str,
    body: &serde_json::Value,
) -> Result<(serde_json::Value, Option<bool>), AppError> {
    match section {
        "site" => {
            let cfg: SiteConfig = deser(body)?;
            cfg.validate()?;
            Ok((to_value(&cfg)?, None))
        }
        "auth" => {
            let cfg: AuthConfig = deser(body)?;
            cfg.validate()?;
            let reg_open = cfg.registration.policy == RegistrationPolicy::Open;
            Ok((to_value(&cfg)?, Some(reg_open)))
        }
        "ratelimit" => {
            let cfg: RateLimitSettings = deser(body)?;
            cfg.validate()?;
            Ok((to_value(&cfg)?, None))
        }
        "audit-config" => {
            let cfg: AuditConfig = deser(body)?;
            cfg.validate()?;
            Ok((to_value(&cfg)?, None))
        }
        "backup-policy" => {
            let cfg: BackupPolicy = deser(body)?;
            cfg.validate()?;
            Ok((to_value(&cfg)?, None))
        }
        _ => Err(AppError::bad_request("UNKNOWN_SECTION", "未知的设置板块")),
    }
}

fn deser<T: for<'de> Deserialize<'de>>(body: &serde_json::Value) -> Result<T, AppError> {
    serde_json::from_value(body.clone())
        .map_err(|e| AppError::bad_request("INVALID_SECTION_BODY", &e.to_string()))
}

fn to_value<T: Serialize>(v: &T) -> Result<serde_json::Value, AppError> {
    serde_json::to_value(v).map_err(|_| AppError::bad_request("SERIALIZE_FAILED", "配置序列化失败"))
}

// ───────────────────────────── 通用校验工具 ─────────────────────────────

/// 仅做格式校验的 URL:必须 http(s):// 前缀且非空。空串视为"未设置"由调用方按 Option 处理。
fn is_http_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

// ───────────────────────────── site ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteConfig {
    /// 品牌名称(登录页/邮件/PWA/404)。
    pub brand: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    /// 可选 logo URL。
    #[serde(default)]
    pub logo_url: Option<String>,
    pub canonical_url: String,
    pub admin_url: String,
    pub cdn_url: String,
    /// IANA 时区,如 Asia/Shanghai。
    pub timezone: String,
    /// BCP-47 语言标签,如 zh-CN。
    pub language: String,
    pub theme: Theme,
    /// 主色调,任意 CSS 颜色字符串(oklch/hex)。
    pub accent: String,
    #[serde(default)]
    pub seo: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    System,
}

impl SiteConfig {
    fn validate(&self) -> Result<(), AppError> {
        if self.brand.trim().is_empty() || self.brand.chars().count() > 80 {
            return Err(invalid("brand 必须非空且 ≤ 80 字符"));
        }
        for (name, url) in [
            ("canonicalUrl", &self.canonical_url),
            ("adminUrl", &self.admin_url),
            ("cdnUrl", &self.cdn_url),
        ] {
            if !is_http_url(url) {
                return Err(invalid(&format!("{name} 必须是 http(s):// URL")));
            }
        }
        if let Some(ref l) = self.logo_url {
            if !l.is_empty() && !is_http_url(l) {
                return Err(invalid("logoUrl 必须是 http(s):// URL"));
            }
        }
        if self.timezone.trim().is_empty() {
            return Err(invalid("timezone 不能为空"));
        }
        if self.language.trim().is_empty() {
            return Err(invalid("language 不能为空"));
        }
        if self.accent.trim().is_empty() {
            return Err(invalid("accent 不能为空"));
        }
        if let Some(ref seo) = self.seo {
            if seo.chars().count() > 500 {
                return Err(invalid("seo 不能超过 500 字符"));
            }
        }
        Ok(())
    }
}

// ───────────────────────────── auth ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub jwt: JwtConfig,
    /// 第三方登录开关:仅存配置(store-only),不在此集成真实 OAuth。
    #[serde(default)]
    pub oauth: OAuthToggles,
    pub registration: RegistrationConfig,
    pub two_factor: TwoFactorConfig,
    pub lockout: LockoutConfig,
    pub password: PasswordPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JwtConfig {
    /// access token 有效期(分钟)。
    pub access_ttl_minutes: u32,
    /// refresh token 有效期(天)。
    pub refresh_ttl_days: u32,
    /// 是否启用刷新轮换。
    #[serde(default)]
    pub rotation: bool,
    pub algorithm: JwtAlgorithm,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JwtAlgorithm {
    Hs256,
    Rs256,
    Es256,
}

/// 各第三方渠道开关。仅存储,不集成真实 OAuth provider。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthToggles {
    #[serde(default)]
    pub google: bool,
    #[serde(default)]
    pub apple: bool,
    #[serde(default)]
    pub wechat: bool,
    #[serde(default)]
    pub qq: bool,
    #[serde(default)]
    pub github: bool,
    /// 邮件魔法链接。
    #[serde(default)]
    pub magic_link: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationConfig {
    pub policy: RegistrationPolicy,
    /// 仅 policy == enterprise_domain 时生效的允许域名(如 @acme.com)。
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationPolicy {
    /// 开放注册(向后兼容:映射 system_settings.registration_enabled = true)。
    Open,
    /// 仅邀请码。
    InviteOnly,
    /// 仅企业域名。
    EnterpriseDomain,
    /// 完全关闭。
    Closed,
}

/// 各角色 2FA 强制档位。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwoFactorConfig {
    pub admin: TwoFactorTier,
    pub paid: TwoFactorTier,
    pub regular: TwoFactorTier,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TwoFactorTier {
    /// 禁用。
    Off,
    /// 可选。
    Optional,
    /// 强制。
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockoutConfig {
    /// 触发锁定的失败次数。
    pub max_failures: u32,
    /// 滑动窗口(分钟)。
    pub window_minutes: u32,
    /// 锁定时长(分钟)。
    pub lock_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordPolicy {
    pub min_length: u32,
    /// zxcvbn 最低评分 0-4。
    pub min_zxcvbn_score: u8,
    /// 禁用最近 N 个历史密码。
    #[serde(default)]
    pub history_count: u32,
}

impl AuthConfig {
    fn validate(&self) -> Result<(), AppError> {
        if !(1..=1440).contains(&self.jwt.access_ttl_minutes) {
            return Err(invalid("jwt.accessTtlMinutes 必须在 1-1440 分钟之间"));
        }
        if !(1..=365).contains(&self.jwt.refresh_ttl_days) {
            return Err(invalid("jwt.refreshTtlDays 必须在 1-365 天之间"));
        }
        if self.registration.policy == RegistrationPolicy::EnterpriseDomain
            && self.registration.allowed_domains.is_empty()
        {
            return Err(invalid("企业域名策略下 allowedDomains 不能为空"));
        }
        if !(1..=1000).contains(&self.lockout.max_failures) {
            return Err(invalid("lockout.maxFailures 必须在 1-1000 之间"));
        }
        if !(1..=1440).contains(&self.lockout.window_minutes) {
            return Err(invalid("lockout.windowMinutes 必须在 1-1440 之间"));
        }
        if !(1..=10080).contains(&self.lockout.lock_minutes) {
            return Err(invalid("lockout.lockMinutes 必须在 1-10080 之间"));
        }
        if !(6..=128).contains(&self.password.min_length) {
            return Err(invalid("password.minLength 必须在 6-128 之间"));
        }
        if self.password.min_zxcvbn_score > 4 {
            return Err(invalid("password.minZxcvbnScore 必须在 0-4 之间"));
        }
        if self.password.history_count > 50 {
            return Err(invalid("password.historyCount 必须 ≤ 50"));
        }
        Ok(())
    }
}

// ───────────────────────────── ratelimit ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitSettings {
    /// 全局 RPS 硬上限。
    pub global_rps: u32,
    /// 匿名每分钟请求数。
    pub anon_rpm: u32,
    /// 已认证每分钟请求数。
    pub auth_rpm: u32,
    /// 付费每分钟请求数。
    pub paid_rpm: u32,
    /// 敏感接口配额(按 path 配置;次数 / 窗口秒 / 维度)。
    #[serde(default)]
    pub sensitive: Vec<SensitiveQuota>,
    /// IP 拉黑阈值。
    pub ip_blocklist: IpBlocklistConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensitiveQuota {
    pub path: String,
    pub limit: u32,
    pub window_secs: u32,
    /// 限流维度:ip 或 uid。
    pub scope: QuotaScope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaScope {
    Ip,
    Uid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpBlocklistConfig {
    /// 每小时 429 次数达阈值后自动拉黑。
    pub threshold_per_hour: u32,
    /// 拉黑时长(小时)。
    pub ban_hours: u32,
}

impl RateLimitSettings {
    fn validate(&self) -> Result<(), AppError> {
        if !(1..=10_000_000).contains(&self.global_rps) {
            return Err(invalid("globalRps 必须在 1-10000000 之间"));
        }
        for (name, v) in [
            ("anonRpm", self.anon_rpm),
            ("authRpm", self.auth_rpm),
            ("paidRpm", self.paid_rpm),
        ] {
            if !(1..=1_000_000).contains(&v) {
                return Err(invalid(&format!("{name} 必须在 1-1000000 之间")));
            }
        }
        for q in &self.sensitive {
            if q.path.trim().is_empty() {
                return Err(invalid("sensitive[].path 不能为空"));
            }
            if q.limit == 0 || q.window_secs == 0 {
                return Err(invalid("sensitive[].limit / windowSecs 必须 > 0"));
            }
        }
        if self.ip_blocklist.threshold_per_hour == 0 {
            return Err(invalid("ipBlocklist.thresholdPerHour 必须 > 0"));
        }
        if !(1..=8760).contains(&self.ip_blocklist.ban_hours) {
            return Err(invalid("ipBlocklist.banHours 必须在 1-8760 之间"));
        }
        Ok(())
    }
}

// ───────────────────────────── audit-config ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditConfig {
    /// 审计日志保留天数。
    pub retention_days: u32,
    /// 是否启用 SHA256 哈希链(不可篡改)。
    #[serde(default)]
    pub hash_chain: bool,
    /// SIEM 转发目标。target 为 none 时 url 可空。
    pub siem: SiemConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiemConfig {
    pub target: SiemTarget,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SiemTarget {
    None,
    Elk,
    Datadog,
    Splunk,
}

impl AuditConfig {
    fn validate(&self) -> Result<(), AppError> {
        if !(1..=3650).contains(&self.retention_days) {
            return Err(invalid("retentionDays 必须在 1-3650 天之间"));
        }
        if self.siem.target != SiemTarget::None {
            match self.siem.url {
                Some(ref u) if is_http_url(u) => {}
                _ => return Err(invalid("启用 SIEM 转发时 url 必须是 http(s):// URL")),
            }
        }
        Ok(())
    }
}

// ───────────────────────────── backup-policy ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPolicy {
    /// 全量备份 cron 表达式。
    pub full_cron: String,
    /// 增量备份 cron 表达式。
    pub incremental_cron: String,
    /// WAL 流式归档开关。
    #[serde(default)]
    pub wal_archive: bool,
    /// 备份目标(多区域)。
    pub targets: Vec<BackupTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTarget {
    /// 目标标识,如 primary / replica / cold。
    pub name: String,
    /// 目标 URI,如 s3://... / glacier://...。
    pub uri: String,
    /// 保留天数。
    pub retention_days: u32,
}

impl BackupPolicy {
    fn validate(&self) -> Result<(), AppError> {
        for (name, cron) in [
            ("fullCron", &self.full_cron),
            ("incrementalCron", &self.incremental_cron),
        ] {
            if !is_valid_cron(cron) {
                return Err(invalid(&format!("{name} 必须是 5 字段 cron 表达式")));
            }
        }
        if self.targets.is_empty() {
            return Err(invalid("targets 不能为空"));
        }
        for t in &self.targets {
            if t.name.trim().is_empty() || t.uri.trim().is_empty() {
                return Err(invalid("targets[].name / uri 不能为空"));
            }
            if !(1..=36500).contains(&t.retention_days) {
                return Err(invalid("targets[].retentionDays 必须在 1-36500 天之间"));
            }
        }
        Ok(())
    }
}

/// 仅做字段数校验的 cron 解析(5 字段:分 时 日 月 周)。不求语义完整,只防明显格式错。
fn is_valid_cron(s: &str) -> bool {
    s.split_whitespace().count() == 5
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn site_round_trips_and_validates() {
        let body = json!({
            "brand": "WordForge",
            "canonicalUrl": "https://wordforge.app",
            "adminUrl": "https://admin.wordforge.app",
            "cdnUrl": "https://cdn.wordforge.app",
            "timezone": "Asia/Shanghai",
            "language": "zh-CN",
            "theme": "system",
            "accent": "oklch(58% 0.20 269)"
        });
        let (out, reg) = parse_typed_section("site", &body).unwrap();
        assert!(reg.is_none());
        assert_eq!(out["brand"], "WordForge");
    }

    #[test]
    fn site_rejects_bad_url() {
        let body = json!({
            "brand": "X", "canonicalUrl": "ftp://x", "adminUrl": "https://a",
            "cdnUrl": "https://c", "timezone": "UTC", "language": "en",
            "theme": "dark", "accent": "#fff"
        });
        assert!(parse_typed_section("site", &body).is_err());
    }

    #[test]
    fn auth_open_policy_maps_registration_open_true() {
        let body = auth_body("open", &[]);
        let (_out, reg) = parse_typed_section("auth", &body).unwrap();
        assert_eq!(reg, Some(true));
    }

    #[test]
    fn auth_closed_policy_maps_registration_open_false() {
        let body = auth_body("closed", &[]);
        let (_out, reg) = parse_typed_section("auth", &body).unwrap();
        assert_eq!(reg, Some(false));
    }

    #[test]
    fn auth_enterprise_domain_requires_domains() {
        let body = auth_body("enterprise_domain", &[]);
        assert!(parse_typed_section("auth", &body).is_err());
        let ok = auth_body("enterprise_domain", &["@acme.com"]);
        assert!(parse_typed_section("auth", &ok).is_ok());
    }

    fn auth_body(policy: &str, domains: &[&str]) -> serde_json::Value {
        json!({
            "jwt": {"accessTtlMinutes": 30, "refreshTtlDays": 30, "algorithm": "RS256"},
            "registration": {"policy": policy, "allowedDomains": domains},
            "twoFactor": {"admin": "required", "paid": "optional", "regular": "optional"},
            "lockout": {"maxFailures": 5, "windowMinutes": 10, "lockMinutes": 30},
            "password": {"minLength": 10, "minZxcvbnScore": 3, "historyCount": 5}
        })
    }

    #[test]
    fn ratelimit_validates_ranges() {
        let body = json!({
            "globalRps": 20000, "anonRpm": 30, "authRpm": 240, "paidRpm": 600,
            "sensitive": [{"path": "POST /auth/login", "limit": 10, "windowSecs": 300, "scope": "ip"}],
            "ipBlocklist": {"thresholdPerHour": 100, "banHours": 24}
        });
        assert!(parse_typed_section("ratelimit", &body).is_ok());
        let bad = json!({
            "globalRps": 0, "anonRpm": 30, "authRpm": 240, "paidRpm": 600,
            "ipBlocklist": {"thresholdPerHour": 100, "banHours": 24}
        });
        assert!(parse_typed_section("ratelimit", &bad).is_err());
    }

    #[test]
    fn audit_siem_requires_url_when_enabled() {
        let off = json!({"retentionDays": 365, "hashChain": true, "siem": {"target": "none"}});
        assert!(parse_typed_section("audit-config", &off).is_ok());
        let bad = json!({"retentionDays": 365, "siem": {"target": "datadog"}});
        assert!(parse_typed_section("audit-config", &bad).is_err());
        let ok =
            json!({"retentionDays": 365, "siem": {"target": "datadog", "url": "https://intake"}});
        assert!(parse_typed_section("audit-config", &ok).is_ok());
    }

    #[test]
    fn backup_validates_cron_and_targets() {
        let ok = json!({
            "fullCron": "0 3 * * 0", "incrementalCron": "0 */6 * * *", "walArchive": true,
            "targets": [{"name": "primary", "uri": "s3://wf-backup", "retentionDays": 30}]
        });
        assert!(parse_typed_section("backup-policy", &ok).is_ok());
        let bad_cron = json!({
            "fullCron": "0 3 *", "incrementalCron": "0 */6 * * *",
            "targets": [{"name": "primary", "uri": "s3://x", "retentionDays": 30}]
        });
        assert!(parse_typed_section("backup-policy", &bad_cron).is_err());
        let no_targets = json!({
            "fullCron": "0 3 * * 0", "incrementalCron": "0 */6 * * *", "targets": []
        });
        assert!(parse_typed_section("backup-policy", &no_targets).is_err());
    }
}
