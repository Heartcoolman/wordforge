//! AMAS 指标看板新端点的 HTTP 集成测试：seed 真实数据 → 打 9 个端点 →
//! 校验路由可达 + camelCase 序列化契约（前端依赖）+ 聚合数值正确。

mod common;

use axum::http::{Method, StatusCode};
use chrono::{Duration, Utc};
use rusqlite::params;

use common::app::{spawn_test_server, TestApp};
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

fn seed(app: &TestApp) {
    let conn = app.state.store().connection().unwrap();
    let now = Utc::now();
    let now_s = now.to_rfc3339();

    // 14 个难度段词
    for i in 0..14 {
        conn.execute(
            "INSERT INTO words (id,text,meaning,difficulty,created_at) VALUES (?1,?2,?3,?4,?5)",
            params![
                format!("w{i}"),
                format!("word{i}"),
                "m",
                (i as f64) / 14.0,
                now_s
            ],
        )
        .unwrap();
    }

    // 用户阶段：(uid, total_event_count, learning_records 条数)
    let users = [
        ("u1", 8i64, 8usize),
        ("u2", 20, 10),
        ("u3", 210, 20),
        ("u4", 300, 5),
        ("u5", 5, 5),
        ("u6", 50, 2),
    ];
    for (uid, total, recs) in users {
        conn.execute(
            "INSERT INTO engine_user_states (user_id,total_event_count,last_active_at,created_at)
             VALUES (?1,?2,?3,?4)",
            params![uid, total, now_s, now_s],
        )
        .unwrap();
        for r in 0..recs {
            conn.execute(
                "INSERT INTO learning_records (user_id,id,word_id,is_correct,response_time_ms,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![uid, format!("{uid}-r{r}"), format!("w{}", r % 14), (r % 2) as i64, 1500 + (r as i64) * 50, now_s],
            )
            .unwrap();
        }
        // ELO + 7d 前快照
        conn.execute(
            "INSERT INTO user_elo (user_id,rating,games) VALUES (?1,?2,?3)",
            params![uid, 1200.0 + (total as f64), total],
        )
        .unwrap();
        let week_ago = (now - Duration::days(8)).format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO user_elo_history (user_id,snapshot_date,rating,games) VALUES (?1,?2,?3,?4)",
            params![uid, week_ago, 1180.0 + (total as f64), total],
        )
        .unwrap();
    }

    // word_learning_states：最近 3 天各用一个用户覆盖 14 词（PK=(user,word)，跨天换用户避免冲突），
    // mastery 随难度递减（forget = 1-mastery 递增），构成 3 天 × 14 段热图。
    for (d, uid) in [(0i64, "u1"), (1, "u2"), (2, "u3")] {
        let day = (now - Duration::days(d)).to_rfc3339();
        for i in 0..14 {
            conn.execute(
                "INSERT INTO word_learning_states (user_id,word_id,state,mastery_level,half_life,updated_at)
                 VALUES (?1,?2,'REVIEWING',?3,24.0,?4)",
                params![uid, format!("w{i}"), 1.0 - (i as f64) / 14.0, day],
            )
            .unwrap();
        }
    }

    // 两个配置版本（含真实可调参 half_life_base_epsilon）
    for (h, eps) in [("vA", 0.30), ("vB", 0.41)] {
        conn.execute(
            "INSERT INTO amas_config_versions
             (version_hash,snapshot_json,author_admin_id,source,created_at)
             VALUES (?1,?2,'admin','manual',?3)",
            params![
                h,
                format!("{{\"memory_model\":{{\"half_life_base_epsilon\":{eps}}}}}"),
                now_s
            ],
        )
        .unwrap();
    }

    // monitoring events：路由分布 + 命中 + 疲劳触发 + 异常 + 两版本
    // (id, algo, is_correct, fatigue, latency, is_anomaly, violations, version)
    type EventRow = (
        &'static str,
        &'static str,
        i64,
        f64,
        i64,
        i64,
        &'static str,
        &'static str,
    );
    let events: [EventRow; 12] = [
        ("e1", "ensemble", 1, 0.2, 3, 0, "[]", "vA"),
        ("e2", "ensemble", 1, 0.9, 4, 0, "[]", "vA"),
        ("e3", "ensemble", 0, 0.3, 6, 0, "[]", "vA"),
        ("e4", "mdm", 1, 0.95, 5, 0, "[]", "vA"),
        ("e5", "swd", 1, 0.1, 2, 0, "[]", "vA"),
        (
            "e6",
            "heuristic",
            0,
            0.4,
            8,
            1,
            r#"[{"field":"fatigue","value":1.2,"expected_range":"[0, 1]"}]"#,
            "vA",
        ),
        ("e7", "ensemble", 1, 0.2, 3, 0, "[]", "vB"),
        ("e8", "ensemble", 1, 0.5, 4, 0, "[]", "vB"),
        ("e9", "mdm", 1, 0.92, 5, 0, "[]", "vB"),
        ("e10", "mdm", 1, 0.15, 4, 0, "[]", "vB"),
        ("e11", "swd", 0, 0.3, 7, 0, "[]", "vB"),
        (
            "e12",
            "heuristic",
            0,
            0.2,
            9,
            1,
            r#"[{"field":"batch_size","value":0,"expected_range":">= 1"}]"#,
            "vB",
        ),
    ];
    for (id, algo, correct, fatigue, lat, anom, viol, ver) in events {
        conn.execute(
            "INSERT INTO engine_monitoring_events
             (id,user_id,session_id,event_type,timestamp,latency_ms,is_anomaly,invariant_violations_json,
              user_state_fatigue,reward_value,config_version,routing_algo,is_correct)
             VALUES (?1,'u1','s1','process_event',?2,?3,?4,?5,?6,0.5,?7,?8,?9)",
            params![id, now_s, lat, anom, viol, fatigue, ver, algo, correct],
        )
        .unwrap();
    }
}

async fn get_json(app: &axum::Router, token: &str, path: &str) -> serde_json::Value {
    let resp = request(
        app,
        Method::GET,
        path,
        None,
        &[("authorization", auth_header(token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "GET {path} 失败: {body}");
    body["data"].clone()
}

#[tokio::test]
async fn it_amas_dashboard_endpoints_real_aggregation() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    seed(&app);

    // 1. 阶段分布
    let d = get_json(
        &app.app,
        &token,
        "/api/admin/amas/metrics/stage-distribution",
    )
    .await;
    assert_eq!(d["totalUsers"], 6);
    assert_eq!(d["stages"].as_array().unwrap().len(), 3);
    assert!(
        !d["trend"].as_array().unwrap().is_empty(),
        "trend 应含当日实时点"
    );
    let cold = d["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["stage"] == "cold")
        .unwrap();
    assert_eq!(cold["users"], 2); // u1,u5

    // 2. ELO 散点（camelCase + 7d Δ）
    let d = get_json(&app.app, &token, "/api/admin/amas/metrics/elo-scatter").await;
    assert_eq!(d["total"], 6);
    let p0 = &d["points"].as_array().unwrap()[0];
    assert!(
        p0.get("elo").is_some() && p0.get("decisions").is_some() && p0.get("deltaElo").is_some()
    );
    assert!(
        (p0["deltaElo"].as_f64().unwrap() - 20.0).abs() < 1e-6,
        "Δ = 当前-8天前 = 20"
    );

    // 3. MDM 热图
    let d = get_json(
        &app.app,
        &token,
        "/api/admin/amas/metrics/mdm-heatmap?days=7",
    )
    .await;
    assert_eq!(d["bandCount"], 14);
    assert!(!d["days"].as_array().unwrap().is_empty());
    assert!(d["peak"].as_f64().unwrap() > 0.0);

    // 4. 疲劳时序
    let d = get_json(
        &app.app,
        &token,
        "/api/admin/amas/metrics/fatigue-timeseries?days=7",
    )
    .await;
    assert!(d.get("threshold").is_some());
    assert!(
        d["totalTriggers"].as_u64().unwrap() >= 3,
        "0.9/0.95/0.92 等应触发"
    );
    assert!(!d["points"].as_array().unwrap().is_empty());

    // 5. 决策直方图
    let d = get_json(
        &app.app,
        &token,
        "/api/admin/amas/metrics/decision-histogram?days=7",
    )
    .await;
    assert_eq!(d["buckets"].as_array().unwrap().len(), 7);
    assert_eq!(d["totalUsers"], 6);

    // 6. 状态流转
    let d = get_json(
        &app.app,
        &token,
        "/api/admin/amas/user-state/transitions?hours=48",
    )
    .await;
    let new_cold = d["transitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["from"] == "new" && t["to"] == "cold")
        .unwrap();
    assert_eq!(new_cold["count"], 2, "u1,u5 新注册→冷");

    // 7. 学习风格聚类
    let d = get_json(&app.app, &token, "/api/admin/amas/user-state/clusters").await;
    assert_eq!(d["k"], 4);
    assert!(d["totalUsers"].as_u64().unwrap() >= 4);
    assert_eq!(d["clusters"].as_array().unwrap().len(), 4);

    // 8. 异常 feed
    let d = get_json(&app.app, &token, "/api/admin/amas/anomalies/feed?days=7").await;
    assert_eq!(d["summary"]["invariantTotal"], 12);
    assert!(
        d["summary"]["error"].as_u64().unwrap() >= 1,
        "batch_size → error"
    );
    assert!(
        d["summary"]["warn"].as_u64().unwrap() >= 1,
        "fatigue 越界 → warn"
    );
    assert_eq!(d["items"].as_array().unwrap().len(), 2);
    assert!(
        d["items"][0].get("impactedUsers").is_some(),
        "异常项应含影响面字段"
    );
    assert!(d["items"][0].get("impactPct").is_some());

    // 9. 版本对比扩展（含 configEpsilon 从 snapshot 读取）
    let d = get_json(
        &app.app,
        &token,
        "/api/admin/amas/compare/ext?versionA=vA&versionB=vB",
    )
    .await;
    assert!(d["a"].get("hitRate").is_some() && d["a"].get("p95LatencyMs").is_some());
    assert!(d["a"].get("ensembleShare").is_some() && d["a"].get("spark").is_some());
    // 对齐设计稿补齐的 7d 留存 + P95 答题完成时间（真实后端聚合）
    assert!(d["a"].get("retention7d").is_some(), "应含 7d 留存字段");
    assert!(
        d["a"].get("p95CompletionMs").is_some(),
        "应含 P95 答题完成时间字段"
    );
    assert_eq!(d["a"]["eventCount"], 6);
    assert!((d["a"]["configEpsilon"].as_f64().unwrap() - 0.30).abs() < 1e-9);
    assert!((d["b"]["configEpsilon"].as_f64().unwrap() - 0.41).abs() < 1e-9);
}
