use axum::extract::{Query, State};
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

async fn get_status(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    // web 版本下发,供 web 端 Service Worker 更新编排:
    //   webTargetVersion   - 单一真相源:当前激活的 web-app@stable 工件版本(=后端实际托管版本)。
    //                        激活即移动该指针,与托管版本物理一致、不漂移。无 active 时回退
    //                        upgrade-policy 的 suggested/min(迁移期不回归)。
    //   webPwaSilentUpdate - 新构建就绪后是否静默切换(true)还是弹提示(false);来自 upgrade-policy,
    //                        与版本号正交;策略未配置时默认 true。
    let policies = state.get_upgrade_policies_cached();
    let web = policies.get("web");
    let policy_target =
        web.and_then(|p| p.suggested_version.clone().or_else(|| p.min_version.clone()));
    let web_pwa_silent = web.map(|p| p.pwa_silent_update).unwrap_or(true);

    let active_webapp = match state
        .run_store_task("status.active_webapp", |store| {
            store.get_active_pack_version(
                "web-app",
                crate::store::operations::resource_packs::ResourcePackChannel::Stable,
            )
        })
        .await
    {
        Ok(Ok(opt)) => opt.map(|v| v.version),
        _ => None, // 查询失败/无 active 不阻塞 status,回退 policy
    };
    let web_target = active_webapp.or(policy_target);

    ok(serde_json::json!({
        "maintenanceMode": state.is_maintenance(),
        "version": env!("GIT_VERSION"),
        "webTargetVersion": web_target,
        "webPwaSilentUpdate": web_pwa_silent,
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
