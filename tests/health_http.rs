mod common;

use axum::http::{Method, StatusCode};
use std::time::Duration;

use common::app::spawn_test_server;
use common::http::{request, response_json};
use learning_backend::store::operations::system_settings::SystemSettings;

#[tokio::test]
async fn it_health_live_and_ready() {
    let app = spawn_test_server().await;

    let live = request(&app.app, Method::GET, "/health/live", None, &[]).await;
    let (live_status, _, _) = response_json(live).await;
    assert_eq!(live_status, StatusCode::OK);

    let ready = request(&app.app, Method::GET, "/health/ready", None, &[]).await;
    let (ready_status, _, _) = response_json(ready).await;
    assert_eq!(ready_status, StatusCode::OK);
}

#[tokio::test]
async fn it_health_database_is_ok() {
    let app = spawn_test_server().await;

    use common::auth::{auth_header, setup_admin_and_get_token};
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let db = request(
        &app.app,
        Method::GET,
        "/health/database",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(db).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["healthy"], true);
}

#[tokio::test]
async fn it_public_health_hides_upstream_url_and_reports_store() {
    let app = spawn_test_server().await;

    let response = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["services"]["store"]["healthy"], true);
    assert!(body["services"]["wordbookCenter"]["healthy"].is_boolean());
    assert!(body["services"]["wordbookCenter"]["probeSkipped"].is_boolean());
    assert!(body["services"]["wordbookCenter"].get("url").is_none());
}

#[tokio::test]
async fn it_public_health_reports_unreachable_wordbook_center() {
    let app = spawn_test_server().await;
    let mut settings: SystemSettings = app.state.store().get_system_settings().unwrap();
    settings.wordbook_center_url = Some("https://10.255.255.1/unreachable".to_string());
    app.state.store().save_system_settings(&settings).unwrap();

    // 探针已改为后台刷新（不阻塞请求）：首个请求触发后台探活，随后轮询直到结果落地。
    // 每次请求都应秒回（不含网络往返），不可达探活在后台 ≤5s 内失败并写回 (false,false)。
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let started_at = tokio::time::Instant::now();
        let response = request(&app.app, Method::GET, "/health", None, &[]).await;
        let elapsed = started_at.elapsed();
        let (status, _, body) = response_json(response).await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            elapsed < Duration::from_secs(2),
            "health 请求不应阻塞在探针网络往返上，实测 {:?}",
            elapsed
        );

        let wbc = &body["services"]["wordbookCenter"];
        if wbc["healthy"] == false {
            assert_eq!(wbc["probeSkipped"], false, "探测确实发生过，probeSkipped 应为 false");
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "后台探针未在期限内将不可达词书中心标记为 unhealthy"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// M0-R4：maintenance 模式集成测试。
///
/// apply swapping 期间 state.set_maintenance(true) 被调用，maintenance_middleware
/// 拦截所有非豁免路径并返回 503。/health 不在豁免列表，应被拦截。
/// apply 完成或回滚后 set_maintenance(false)，/health 恢复 200。
#[tokio::test]
async fn health_returns_503_during_maintenance_and_200_after() {
    let app = spawn_test_server().await;

    // 正常状态：/health 返回 200
    let resp = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "初始状态 /health 应为 200");

    // 模拟 apply swapping 阶段开启 maintenance 模式
    app.state.set_maintenance(true);

    // maintenance 期间：/health 应返回 503，body 含 MAINTENANCE code
    let resp = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "maintenance 期间 /health 应为 503，实际：{status}"
    );
    assert_eq!(
        body["maintenance"], true,
        "maintenance 响应 body 应含 maintenance=true，实际：{body}"
    );

    // 模拟 apply 完成（新进程启动清理 flag）或回滚后关闭 maintenance
    app.state.set_maintenance(false);

    // 恢复后：/health 重新返回 200
    let resp = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "maintenance 解除后 /health 应恢复 200"
    );
}
