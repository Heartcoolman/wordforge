/// M0-R4 集成测试：updater apply swapping 期间 maintenance 模式的自动切换。
///
/// apply_locked() 进入 Swapping 阶段时调 on_maintenance(true)，
/// apply 完成或回滚后调 on_maintenance(false)。
/// on_maintenance 回调体为 state.set_maintenance(active)。
/// 本文件验证该回调的前后效果：/health 在 maintenance 期间返回 503，恢复后 200。
mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::http::{request, response_json};

/// 验证 on_maintenance(true) 的效果：/health 返回 503 + body 含 maintenance=true。
/// 对应 apply_locked() Swapping 开始时的调用路径。
#[tokio::test]
async fn health_503_when_on_maintenance_callback_fires_true() {
    let app = spawn_test_server().await;

    // 前置检查：正常态 /health 200
    let resp = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "前置：/health 初始应为 200");

    // 模拟 on_maintenance 回调 → state.set_maintenance(true)
    app.state.set_maintenance(true);

    // 验证 is_maintenance() 状态与回调一致
    assert!(
        app.state.is_maintenance(),
        "set_maintenance(true) 后 is_maintenance() 应为 true"
    );

    // 验证 /health 路由：maintenance 期间返回 503
    let resp = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "on_maintenance(true) 后 /health 应为 503，实际：{status}"
    );
    assert_eq!(
        body["maintenance"], true,
        "503 响应 body 应含 maintenance=true，实际：{body}"
    );
}

/// 验证 on_maintenance(false) 的效果：apply 完成或回滚后 /health 恢复 200。
/// 对应 apply_locked() 成功路径（health check 通过后）或回滚路径的调用。
#[tokio::test]
async fn health_recovers_200_when_on_maintenance_callback_fires_false() {
    let app = spawn_test_server().await;

    // 先激活 maintenance 模式
    app.state.set_maintenance(true);
    assert!(
        app.state.is_maintenance(),
        "set_maintenance(true) 后应为 true"
    );

    // 模拟 apply 完成 → on_maintenance(false)
    app.state.set_maintenance(false);

    // 验证 is_maintenance() 归零
    assert!(
        !app.state.is_maintenance(),
        "set_maintenance(false) 后 is_maintenance() 应为 false"
    );

    // 验证 /health 恢复 200
    let resp = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "on_maintenance(false) 后 /health 应恢复 200，实际：{status}"
    );
}

/// 验证 maintenance 模式不影响 /health/live（liveness 探针无状态，始终 200）。
/// liveness 探针用于进程存活检测，不应被 maintenance 拦截。
#[tokio::test]
async fn liveness_probe_unaffected_by_maintenance_mode() {
    let app = spawn_test_server().await;

    app.state.set_maintenance(true);

    let resp = request(&app.app, Method::GET, "/health/live", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "maintenance 期间 /health/live 仍应 200（liveness 探针不受影响）"
    );
}

/// 验证 is_maintenance() 在 set_maintenance 多次切换后状态始终与最后一次调用一致。
/// 对应 apply_locked() 可能多次切换的边界情况（如回滚重试）。
#[tokio::test]
async fn maintenance_flag_toggles_correctly_across_multiple_switches() {
    let app = spawn_test_server().await;

    // 初始 false
    assert!(!app.state.is_maintenance(), "初始 maintenance 应为 false");

    // true → false → true → false
    app.state.set_maintenance(true);
    assert!(app.state.is_maintenance(), "第一次 true 后应为 true");

    app.state.set_maintenance(false);
    assert!(!app.state.is_maintenance(), "第一次 false 后应为 false");

    app.state.set_maintenance(true);
    assert!(app.state.is_maintenance(), "第二次 true 后应为 true");

    app.state.set_maintenance(false);
    assert!(!app.state.is_maintenance(), "第二次 false 后应为 false");

    // 最终 /health 应为 200
    let resp = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "多次切换后 /health 最终应为 200");
}
