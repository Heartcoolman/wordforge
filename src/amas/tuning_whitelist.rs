//! AMAS LLM 调参白名单：仅这 11 个 path 允许通过 LLM 建议 / 自动应用渠道修改。
//!
//! 任何 patch 路径不在白名单 或 值不在 `(min_safe, max_safe)` 内 → 拒绝。
//! 设计意图：把 LLM 的"宇宙级误改"风险关进 11 个核心维度。
//! 其它参数仍可走人工渠道（PUT /api/admin/amas/config）。

/// 白名单条目：(json-pointer-style path, min_safe, max_safe)
/// path 用点分式，数组下标用 `w[idx]` —— 与前端 schema.ts 一致。
pub struct WhitelistEntry {
    pub path: &'static str,
    pub min_safe: f64,
    pub max_safe: f64,
}

/// Tier-A 11 维白名单。范围比前端 schema.ts 略保守，避免极端值。
pub const TIER_A_WHITELIST: &[WhitelistEntry] = &[
    // FSRS-5 初始稳定性
    WhitelistEntry {
        path: "memoryModel.w[0]",
        min_safe: 0.05,
        max_safe: 3.0,
    },
    WhitelistEntry {
        path: "memoryModel.w[1]",
        min_safe: 0.1,
        max_safe: 3.0,
    },
    WhitelistEntry {
        path: "memoryModel.w[2]",
        min_safe: 0.5,
        max_safe: 15.0,
    },
    WhitelistEntry {
        path: "memoryModel.w[3]",
        min_safe: 1.0,
        max_safe: 25.0,
    },
    // 稳定性增长
    WhitelistEntry {
        path: "memoryModel.w[8]",
        min_safe: 0.5,
        max_safe: 3.0,
    },
    WhitelistEntry {
        path: "memoryModel.w[9]",
        min_safe: 0.05,
        max_safe: 0.8,
    },
    WhitelistEntry {
        path: "memoryModel.w[10]",
        min_safe: 0.1,
        max_safe: 1.6,
    },
    // 评级 bonus
    WhitelistEntry {
        path: "memoryModel.w[15]",
        min_safe: 0.05,
        max_safe: 0.5,
    },
    WhitelistEntry {
        path: "memoryModel.w[16]",
        min_safe: 1.0,
        max_safe: 6.0,
    },
    // 留存目标 + 间隔上限
    WhitelistEntry {
        path: "memoryModel.baseDesiredRetention",
        min_safe: 0.75,
        max_safe: 0.95,
    },
    WhitelistEntry {
        path: "memoryModel.maxIntervalDays",
        min_safe: 30.0,
        max_safe: 365.0,
    },
];

/// 运行期可解析的白名单条目（owned，来自 store row 或 const）。
struct ResolvedEntry {
    min_safe: f64,
    max_safe: f64,
}

/// 从 store 读取白名单为 path→区间 map；表为空或出错时回退 const `TIER_A_WHITELIST`。
fn resolve_whitelist(
    store: &crate::store::Store,
) -> std::collections::HashMap<String, ResolvedEntry> {
    match store.list_tuning_whitelist() {
        Ok(rows) if !rows.is_empty() => rows
            .into_iter()
            .map(|r| {
                (
                    r.path,
                    ResolvedEntry {
                        min_safe: r.min_safe,
                        max_safe: r.max_safe,
                    },
                )
            })
            .collect(),
        Ok(_) => const_fallback(),
        Err(e) => {
            tracing::warn!(error = %e, "读取白名单表失败，回退 const TIER_A_WHITELIST");
            const_fallback()
        }
    }
}

fn const_fallback() -> std::collections::HashMap<String, ResolvedEntry> {
    TIER_A_WHITELIST
        .iter()
        .map(|e| {
            (
                e.path.to_string(),
                ResolvedEntry {
                    min_safe: e.min_safe,
                    max_safe: e.max_safe,
                },
            )
        })
        .collect()
}

/// const-only 查找（保留供无 store 上下文场景）。
pub fn find(path: &str) -> Option<&'static WhitelistEntry> {
    TIER_A_WHITELIST.iter().find(|e| e.path == path)
}

/// 校验 patch 中所有 path / value 都通过白名单 + 范围检查（store 驱动，const fallback）。
/// 返回错误描述 vec；为空表示通过。
pub fn validate_patch(
    store: &crate::store::Store,
    patch: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    let whitelist = resolve_whitelist(store);
    let mut errors = Vec::new();
    for (path, value) in patch {
        let Some(entry) = whitelist.get(path.as_str()) else {
            errors.push(format!("path 不在白名单：{path}"));
            continue;
        };
        let Some(v) = value.as_f64() else {
            errors.push(format!("path={path} 值非数字"));
            continue;
        };
        if v < entry.min_safe || v > entry.max_safe {
            errors.push(format!(
                "path={path} 值 {v} 越界（安全区间 [{}, {}]）",
                entry.min_safe, entry.max_safe
            ));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn whitelist_size_is_11() {
        assert_eq!(TIER_A_WHITELIST.len(), 11);
    }

    #[test]
    fn validate_accepts_in_range_known_paths() {
        // 不 seed → const fallback 路径
        let store = crate::store::Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let patch = json!({
            "memoryModel.baseDesiredRetention": 0.85,
            "memoryModel.w[2]": 3.0,
        });
        let errs = validate_patch(&store, patch.as_object().unwrap());
        assert!(errs.is_empty(), "errors: {:?}", errs);
    }

    #[test]
    fn validate_rejects_unknown_path() {
        let store = crate::store::Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let patch = json!({"ensemble.baseWeightHeuristic": 0.5});
        let errs = validate_patch(&store, patch.as_object().unwrap());
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("白名单"));
    }

    #[test]
    fn validate_rejects_out_of_range() {
        let store = crate::store::Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        let patch = json!({"memoryModel.baseDesiredRetention": 0.5});
        let errs = validate_patch(&store, patch.as_object().unwrap());
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("越界"));
    }

    fn seeded_store() -> crate::store::Store {
        let s = crate::store::Store::open(":memory:", 5000, 1).unwrap();
        s.run_migrations().unwrap();
        s.seed_tuning_whitelist_if_empty().unwrap();
        s
    }

    #[test]
    fn store_backed_accepts_in_range() {
        let store = seeded_store();
        let patch = json!({ "memoryModel.baseDesiredRetention": 0.85 });
        let errs = validate_patch(&store, patch.as_object().unwrap());
        assert!(errs.is_empty(), "errors: {:?}", errs);
    }

    #[test]
    fn store_backed_rejects_unknown_and_out_of_range() {
        let store = seeded_store();
        let unknown = json!({ "ensemble.baseWeightHeuristic": 0.5 });
        assert_eq!(
            validate_patch(&store, unknown.as_object().unwrap()).len(),
            1
        );
        let oob = json!({ "memoryModel.baseDesiredRetention": 0.5 });
        let e = validate_patch(&store, oob.as_object().unwrap());
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("越界"));
    }

    #[test]
    fn store_backed_honors_runtime_added_path() {
        let store = seeded_store();
        // 运行期新增一条白名单 → 之前越界的 path 现在合法
        store
            .insert_tuning_whitelist("memoryModel.w[5]", 0.0, 10.0, "admin")
            .unwrap();
        let patch = json!({ "memoryModel.w[5]": 5.0 });
        assert!(validate_patch(&store, patch.as_object().unwrap()).is_empty());
    }
}
