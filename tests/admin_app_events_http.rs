//! /api/admin/app-events/*（m073 埋点分析读端点）冒烟：seed raw + rollup 后 8 端点全 200，
//! 关键形状与当日 raw 补齐（UNION）口径校验。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

/// seed：今天 2 条 behavior + 1 条 error（走当日 raw 路径），昨天 rollup 一天
/// （3 条 perf api_rtt + 1 用户），驱动 UNION 合并口径。
fn seed(store: &learning_backend::store::Store) {
    let conn = store.connection().unwrap();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();
    for (id, user, cat, name, props, day) in [
        ("a1", "u1", "behavior", "session_start", None, &today),
        (
            "a2",
            "u1",
            "behavior",
            "screen_view",
            Some(r#"{"screen":"study"}"#),
            &today,
        ),
        (
            "a3",
            "u2",
            "error",
            "app_error",
            Some(r#"{"signature":"deadbeef","kind":"js_error"}"#),
            &today,
        ),
    ] {
        conn.execute(
            "INSERT INTO app_events (device_id, user_id, platform, app_version, category, name,
                 client_event_id, client_ts_ms, event_day, props_json)
             VALUES ('d1', ?1, 'web', '1.0.0', ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![user, cat, name, id, now_ms, day, props],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO app_event_daily (day, platform, category, name, count, users, p50_ms, p95_ms, p99_ms)
         VALUES (?1, 'web', 'perf', 'api_rtt', 3, 1, 100, 300, 300)",
        rusqlite::params![yesterday],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO app_user_daily (day, user_id, platform, events) VALUES (?1, 'u1', 'web', 3)",
        rusqlite::params![yesterday],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO app_user_first_seen (user_id, first_day, platform) VALUES ('u1', ?1, 'web')",
        rusqlite::params![yesterday],
    )
    .unwrap();
}

#[tokio::test]
async fn it_admin_app_events_endpoints_smoke() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    seed(app.state.store());
    let auth = [("authorization", auth_header(&admin_token))];

    // overview：当日 raw 计入（totalEvents ≥3）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/app-events/overview?days=7",
        None,
        &auth,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["totalEvents"]["value"].as_f64().unwrap() >= 3.0);
    assert_eq!(body["data"]["errorCount"]["value"], 1.0);

    // trend：今天行存在且 behavior=2
    let (status, _, body) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/admin/app-events/trend?days=7",
            None,
            &auth,
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let rows = body["data"]["rows"].as_array().unwrap();
    let today_row = rows.iter().find(|r| r["day"] == today.as_str()).unwrap();
    assert_eq!(today_row["behavior"], 2);
    assert_eq!(today_row["dau"], 2);

    // top-events / errors / perf / funnel / retention-matrix / activity 全 200
    for path in [
        "/api/admin/app-events/top-events?days=7&category=behavior",
        "/api/admin/app-events/errors?days=7",
        "/api/admin/app-events/perf?days=7&name=api_rtt",
        "/api/admin/app-events/funnel?days=7",
        "/api/admin/app-events/retention-matrix?weeks=4",
        "/api/admin/app-events/activity?days=7",
    ] {
        let (status, _, body) =
            response_json(request(&app.app, Method::GET, path, None, &auth).await).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
    }

    // perf：昨天 rollup 行透出
    let (_, _, body) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/admin/app-events/perf?days=7&name=api_rtt",
            None,
            &auth,
        )
        .await,
    )
    .await;
    let rows = body["data"]["rows"].as_array().unwrap();
    assert!(rows.iter().any(|r| r["p95Ms"] == 300));

    // errors：signature 分组 + 样本
    let (_, _, body) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/admin/app-events/errors?days=7",
            None,
            &auth,
        )
        .await,
    )
    .await;
    let rows = body["data"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["signature"], "deadbeef");
    assert_eq!(rows[0]["samples"][0]["kind"], "js_error");

    // funnel：screen_view(study) 步 count=1
    let (_, _, body) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/admin/app-events/funnel?days=7",
            None,
            &auth,
        )
        .await,
    )
    .await;
    let steps = body["data"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 5);
    assert_eq!(steps[1]["key"], "study_screen");
    assert_eq!(steps[1]["count"], 1);

    // 非 admin token 拒绝
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/app-events/overview?days=7",
        None,
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
