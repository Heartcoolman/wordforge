use axum::extract::{Query, State};
use axum::http::{header, HeaderMap};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_status))
        .route("/device-ban", get(get_device_ban))
}

async fn get_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl axum::response::IntoResponse {
    // web 版本下发,供 web 端 Service Worker 更新编排:
    //   webTargetVersion   - 单一真相源:当前激活的 web-app@stable 工件版本(=后端实际托管版本)。
    //                        激活路径保证「current 符号链接先切成功、DB 指针后翻转」,故该值恒指向
    //                        后端确已托管的构建,不会把客户端导向不存在的版本。**不**回退 upgrade-policy:
    //                        policy 的 suggested/min 不保证有对应已托管工件,回退会重新打开「导向不存在构建」
    //                        路径(与本不变式冲突)。无 active 时返回 null(诚实表示「无服务端托管目标」)。
    //   webPwaSilentUpdate - 新构建就绪后是否静默切换(true)还是弹提示(false);来自 upgrade-policy,
    //                        与版本号正交;策略未配置时默认 true。
    let policies = state.get_upgrade_policies_cached();
    let web = policies.get("web");
    let web_pwa_silent = web.map(|p| p.pwa_silent_update).unwrap_or(true);

    let web_target = match state
        .run_store_task("status.active_webapp", |store| {
            store.get_active_pack_version(
                "web-app",
                crate::store::operations::resource_packs::ResourcePackChannel::Stable,
            )
        })
        .await
    {
        Ok(Ok(opt)) => opt.map(|v| v.version),
        _ => None, // 查询失败/无 active 不阻塞 status
    };

    // 强制升级的权威 HTTP 兜底:native 客户端据此在启动/回前台时校正强升锁——
    // 低于全局版本门(min_client_version)即 forceUpgrade=true(置锁),否则 false(清锁,
    // 即 admin 降低/撤销版本门或客户端已升级后自动恢复)。版本门未启用/未配置时恒 false。
    // 版本取自 UA 中的 WordForge-<platform>/<semver>(与 strict-mode 硬切流同源);
    // web 端不走 UA semver 门(浏览器禁设 UA),故恒 false,不在此承担强升。
    let gate = state.get_version_gate();
    let strict_enabled = state.config().strict_mode.enabled;
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let mut force_upgrade = false;
    if gate.enabled || strict_enabled {
        if let (Some(min), Some((_plat, ver))) = (
            gate.min_client_version.as_deref(),
            ua.and_then(crate::middleware::strict_mode::parse_wordforge_ua),
        ) {
            if let (Ok(c), Ok(m)) =
                (semver::Version::parse(&ver), semver::Version::parse(min))
            {
                force_upgrade = c < m;
            }
        }
    }
    let latest_version = if force_upgrade {
        gate.min_client_version.clone()
    } else {
        None
    };

    ok(serde_json::json!({
        "maintenanceMode": state.is_maintenance(),
        "version": env!("GIT_VERSION"),
        "webTargetVersion": web_target,
        "webPwaSilentUpdate": web_pwa_silent,
        "forceUpgrade": force_upgrade,
        "latestVersion": latest_version,
    }))
}

#[derive(Deserialize)]
struct DeviceBanQuery {
    #[serde(rename = "deviceId")]
    device_id: String,
}

async fn get_device_ban(
    Query(q): Query<DeviceBanQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let device_id = q.device_id;
    let banned = state
        .run_store_task("status.get_device_ban", move |store| {
            store.is_device_banned(&device_id)
        })
        .await??;
    Ok(ok(serde_json::json!({ "banned": banned })))
}
