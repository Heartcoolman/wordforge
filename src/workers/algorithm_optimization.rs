//! B45: Algorithm optimization worker
//! Daily at 00:00, run algorithm parameter optimization cycle.

use std::sync::Arc;

use crate::amas::engine::AMASEngine;
use crate::store::Store;

pub async fn run(store: &Store, engine: &Arc<AMASEngine>) {
    tracing::info!("Algorithm optimization worker running");

    let now = chrono::Utc::now();
    let yesterday = now - chrono::Duration::days(1);
    let store_for_aggregate = store.clone();

    let (total_records, total_correct) = match crate::blocking::run_blocking(
        "worker.algorithm_optimization.aggregate_records",
        move || store_for_aggregate.aggregate_records_since(yesterday),
    )
    .await
    {
        Ok(Ok(stats)) => stats,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "Algorithm optimization: failed to aggregate records");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "Algorithm optimization aggregate task failed");
            return;
        }
    };

    let overall_accuracy = if total_records > 0 {
        total_correct as f64 / total_records as f64
    } else {
        0.0
    };

    tracing::info!(
        total_records,
        total_correct,
        overall_accuracy = format!("{:.3}", overall_accuracy),
        "Algorithm optimization: collected aggregate statistics"
    );

    // E4: Simple parameter adjustment based on overall accuracy
    if total_records >= 50 {
        // #35:读取时记录基线 hash,写回前再校验,杜绝与并发 admin/LLM save 的 lost-update。
        let base = engine.get_config();
        let base_hash = crate::amas::monitoring::compute_config_hash(&base);
        let mut config = base.clone();
        let mut adjusted = false;

        if overall_accuracy < 0.4 {
            let old = config.constraints.max_difficulty_when_fatigued;
            config.constraints.max_difficulty_when_fatigued = (old - 0.05).max(0.2);
            if (config.constraints.max_difficulty_when_fatigued - old).abs() > f64::EPSILON {
                tracing::info!(
                    old = format!("{:.3}", old),
                    new = format!("{:.3}", config.constraints.max_difficulty_when_fatigued),
                    "Algorithm optimization: lowered max_difficulty_when_fatigued due to low accuracy"
                );
                adjusted = true;
            }
        }

        if overall_accuracy > 0.85 {
            let old = config.constraints.max_difficulty_when_fatigued;
            config.constraints.max_difficulty_when_fatigued = (old + 0.03).min(0.9);
            if (config.constraints.max_difficulty_when_fatigued - old).abs() > f64::EPSILON {
                tracing::info!(
                    old = format!("{:.3}", old),
                    new = format!("{:.3}", config.constraints.max_difficulty_when_fatigued),
                    "Algorithm optimization: raised max_difficulty_when_fatigued due to high accuracy"
                );
                adjusted = true;
            }
        }

        if adjusted {
            persist_tuned_config(store, engine, config, &base_hash).await;
        }
    }

    let date = now.format("%Y-%m-%d").to_string();
    let metrics = serde_json::json!({
        "date": date,
        "totalRecords": total_records,
        "totalCorrect": total_correct,
        "overallAccuracy": overall_accuracy,
        "optimizationRun": true,
    });

    let store_for_metrics = store.clone();
    match crate::blocking::run_blocking("worker.algorithm_optimization.store_metrics", move || {
        store_for_metrics.upsert_metrics_daily(&date, "optimization", &metrics)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "Failed to store optimization metrics"),
        Err(e) => tracing::warn!(error = %e, "Algorithm optimization metrics task failed"),
    }
}

/// #12/#35:把 worker 的调参落到与 admin 同一条持久化通路(热重载 + 写 TOML + 落版本表),
/// 使改动跨重启/文件热重载存活,且在版本表留痕可审计/回滚。
///
/// 原子性(#35):重载前再读一次引擎 config,若其 hash 与读取时的 `base_hash` 不一致,
/// 说明期间有并发 admin/LLM save 替换了 Arc——放弃本次写回,避免把基于陈旧快照的改动
/// 覆盖掉对方的全新配置(lost-update)。worker 是低频日任务,跳过等下次即可。
async fn persist_tuned_config(
    store: &Store,
    engine: &Arc<AMASEngine>,
    new_config: crate::amas::config::AMASConfig,
    base_hash: &str,
) {
    // CAS:确认引擎当前仍是读取时的快照,否则有并发写入,放弃。
    let current_hash = crate::amas::monitoring::compute_config_hash(&engine.get_config());
    if current_hash != base_hash {
        tracing::warn!(
            "Algorithm optimization: config changed concurrently, skipping tuning write to avoid lost-update"
        );
        return;
    }

    if let Err(e) = engine.reload_config(new_config.clone()) {
        tracing::warn!(error = %e, "Algorithm optimization: failed to update config");
        return;
    }

    // 写回 TOML:与 config_watcher / startup 的加载源保持一致,改动跨重启存活。
    // 路径解析与 Config::from_env 同源(AMAS_CONFIG_FILE),缺省 amas_config.toml。
    let toml_path = std::env::var("AMAS_CONFIG_FILE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "amas_config.toml".to_string());
    if let Err(e) = new_config.write_to_toml(&toml_path) {
        tracing::warn!(path = %toml_path, error = %e, "Algorithm optimization: failed to write config TOML");
    }

    // 落版本表:复用现有 source 枚举(机器自动应用归 LlmAuto),note 标注来源以便审计区分。
    let snapshot_json = match serde_json::to_string(&new_config) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "Algorithm optimization: failed to serialize config snapshot");
            return;
        }
    };
    let store = store.clone();
    let result = crate::blocking::run_blocking(
        "worker.algorithm_optimization.insert_version",
        move || {
            let parent = store
                .list_amas_config_versions(1)
                .ok()
                .and_then(|mut v| v.pop())
                .map(|r| r.version_hash);
            store.insert_amas_config_version(
                &snapshot_json,
                "algorithm_optimization",
                crate::store::operations::amas_versions::ConfigVersionSource::LlmAuto,
                Some("auto-tuned by algorithm_optimization worker"),
                parent.as_deref(),
            )
        },
    )
    .await;
    match result {
        Ok(Ok((version_id, version_hash))) => tracing::info!(
            version_id,
            version_hash = %version_hash,
            "Algorithm optimization: persisted auto-tuned config version"
        ),
        Ok(Err(e)) => tracing::warn!(error = %e, "Algorithm optimization: failed to insert config version"),
        Err(e) => tracing::warn!(error = %e, "Algorithm optimization: insert version task failed"),
    }
}
