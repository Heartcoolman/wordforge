# 测试补充计划

## 现状

- **总行覆盖率**: 40.53% (4,301 / 10,612 行)
- **现有测试**: 66 个 (41 单元 + 25 集成/属性)
- **测试代码**: 874 行 (源码 9,776 行, 测试/源码比 = 0.09)
- **目标覆盖率**: 75%+ (需新增约 150-180 个测试)

## 优先级与阶段

### P0: Store 操作层 (覆盖率 0-56% → 目标 85%)

Store 层是所有业务逻辑的基础，bug 影响面最大。现有 fixtures 已能创建 user/word/engine_state，扩展成本低。

#### 新增文件: `src/store/operations/` 各模块内 `#[cfg(test)] mod tests`

| 文件 | 当前覆盖 | 需新增测试 | 测试用例 |
|------|---------|-----------|---------|
| `admins.rs` | 0% | 5 | create_admin_success, create_admin_duplicate_email_conflict, create_admin_rollback_on_insert_failure, get_admin_by_id, any_admin_exists_true_and_false |
| `wordbooks.rs` | 0% | 8 | upsert_wordbook, get_wordbook, list_system_wordbooks, list_user_wordbooks, add_word_to_wordbook, remove_word_from_wordbook, list_wordbook_words, count_wordbook_words_consistency |
| `study_configs.rs` | 0% | 3 | set_and_get_study_config, get_missing_returns_none, overwrite_existing_config |
| `learning_sessions.rs` | 8% | 5 | create_and_get_session, create_session_atomicity, get_active_sessions, close_active_sessions, update_session |
| `word_states.rs` | 16% | 6 | set_and_get_state, batch_get_states, get_due_words, get_stats, delete_state, list_user_states |
| `words.rs` | 30% | 4 | search_words, delete_word_cascades, get_words_without_embedding, count_words |
| `users.rs` | 43% | 3 | update_user_email_uniqueness, update_user_profile, list_users_pagination |
| `records.rs` | 56% | 2 | get_user_word_records_filter, count_stats_accuracy |
| `sessions.rs` | 52% | 3 | create_session_atomic, delete_admin_session_atomic, admin_session_crud |

**小计: ~39 个测试**

---

### P1: 路由集成测试 (覆盖率 6-25% → 目标 70%)

复用现有 `tests/common/` 框架 (spawn_test_server + request + response_json)，每个路由模块一个测试文件。

#### 新增文件: `tests/` 目录

| 测试文件 | 对应路由 | 当前覆盖 | 需新增测试 | 关键用例 |
|---------|---------|---------|-----------|---------|
| `learning_http.rs` | `routes/learning.rs` | 6% | 6 | start_session, close_session, get_active_session, get_next_word, submit_answer, get_session_history |
| `wordbooks_http.rs` | `routes/wordbooks.rs` | 11% | 7 | list_system, create_user_wordbook, add_words, remove_word, list_words, ownership_check_403, delete_wordbook |
| `word_states_http.rs` | `routes/word_states.rs` | 15% | 5 | batch_query, batch_query_cap_200, due_list, batch_update, batch_update_cap_500 |
| `user_profile_http.rs` | `routes/user_profile.rs` | 9% | 6 | get_profile, set_reward_preference, set_reward_invalid_type_400, set_habit_profile, set_habit_invalid_hours_400, upload_avatar |
| `study_config_http.rs` | `routes/study_config.rs` | 10% | 3 | get_config, set_config, update_partial_config |
| `content_http.rs` | `routes/content.rs` | 9% | 4 | get_morphemes, set_morphemes, get_etymology, get_confusion_pairs |
| `notifications_http.rs` | `routes/notifications.rs` | 8% | 4 | list_notifications, mark_read, mark_all_read, delete_notification |
| `v1_http.rs` | `routes/v1.rs` | 14% | 4 | list_words_v1, list_records_with_offset, create_session_v1, get_stats_v1 |
| `admin_auth_http.rs` | `routes/admin/auth.rs` | 11% | 4 | admin_setup, admin_setup_already_exists, admin_login, admin_login_wrong_password |
| `admin_http.rs` | `routes/admin/*.rs` | 10-30% | 8 | admin_stats, admin_list_users, analytics_overview, analytics_user_engagement, broadcast_send, monitoring_status, settings_get, settings_update |

**小计: ~51 个测试**

---

### P2: Worker 单元测试 (覆盖率 0% → 目标 60%)

Worker 测试策略: 创建带有预置数据的临时 Store，直接调用 worker 函数，验证副作用。

#### 新增文件: 各 worker 模块内 `#[cfg(test)] mod tests`

| Worker | 需新增测试 | 关键用例 |
|--------|-----------|---------|
| `session_cleanup.rs` | 2 | expired_sessions_removed, active_sessions_kept |
| `metrics_flush.rs` | 2 | metrics_flushed_to_store, empty_metrics_no_write |
| `cache_cleanup.rs` | 2 | old_cache_entries_removed, fresh_entries_kept |
| `delayed_reward.rs` | 2 | due_words_get_reward_update, not_due_words_unchanged |
| `daily_aggregation.rs` | 2 | aggregation_creates_daily_summary, idempotent_rerun |
| `forgetting_alert.rs` | 2 | at_risk_words_generate_notification, serialization_error_skipped |
| `log_export.rs` | 2 | export_writes_to_file, empty_records_no_file |
| `confusion_pair_cache.rs` | 2 | pairs_cached_correctly, serialization_error_skipped |
| `etymology_generation.rs` | 2 | mock_llm_generates_etymology, serialization_error_skipped |
| `health_analysis.rs` | 2 | analysis_produces_report, empty_users_no_crash |
| `weekly_report.rs` | 2 | report_generated_for_active_users, no_users_no_report |
| `word_clustering.rs` | 2 | words_grouped_by_similarity, insufficient_words_no_crash |
| `algorithm_optimization.rs` | 1 | optimization_runs_without_panic |
| `embedding_generation.rs` | 1 | stub_generates_no_embedding |
| `llm_advisor.rs` | 1 | stub_returns_ok |
| `monitoring_aggregate.rs` | 1 | stub_returns_ok |

**小计: ~28 个测试**

#### Worker mod.rs (覆盖率 19% → 50%)

| 需新增测试 | 用例 |
|-----------|------|
| 3 | overlap_guard_skips_concurrent_run, timeout_cancels_long_running_worker, planned_jobs_matches_registered_count |

---

### P3: AMAS 补充测试 (覆盖率 37-78% → 目标 85%)

| 文件 | 当前覆盖 | 需新增测试 | 关键用例 |
|------|---------|-----------|---------|
| `metrics.rs` | 37% | 4 | record_call_increments_counters, snapshot_and_reset_atomicity, concurrent_record_calls, error_count_tracking |
| `metrics_persistence.rs` | 0% | 3 | flush_writes_to_store, flush_additive_merge, restore_from_store_placeholder |
| `engine.rs` | 72% | 5 | process_event_full_pipeline, negative_focus_loss_clamped, engagement_score_bounded, reward_calculation_edge_cases, disabled_algorithm_excluded |
| `elo.rs` | 70% | 2 | k_factor_transition, zpd_boundary_cases |
| `monitoring.rs` | 78% | 2 | invariant_nan_detection, constraints_satisfied_flag_accuracy |
| `memory/iad.rs` | 0% | 3 | record_confusion, interference_penalty_calculation, decay_over_time |
| `memory/evm.rs` | 0% | 2 | encoding_variability_increases_with_diversity, bonus_calculation |
| `memory/mtp.rs` | 0% | 2 | morpheme_transfer_positive, no_shared_morphemes_no_bonus |

**小计: ~23 个测试**

---

### P4: 属性测试扩展 (proptest)

在 `tests/property_*.rs` 中扩展:

| 文件 | 需新增测试 | 用例 |
|------|-----------|------|
| `property_amas_invariants.rs` | 3 | swd_output_in_valid_ranges, ige_ucb_always_positive, trust_scores_bounded_0_1 |
| `property_memory_models.rs` | 2 | iad_penalty_bounded, evm_bonus_non_negative |
| `property_store.rs` (新) | 3 | user_crud_roundtrip, record_key_ordering_invariant, session_create_delete_idempotent |

**小计: ~8 个测试**

---

### P5: 边界与错误路径 (提高现有文件覆盖)

补充已有测试文件中缺失的错误路径:

| 文件 | 需新增测试 | 用例 |
|------|-----------|------|
| `auth_http.rs` | 4 | register_short_password_400, register_invalid_email_400, login_wrong_password_401, login_nonexistent_user_401 |
| `words_http.rs` | 3 | create_word_missing_fields_400, batch_create_over_500_rejected, update_nonexistent_word_404 |
| `records_http.rs` | 3 | batch_create_over_500_rejected, enhanced_stats_returns_data, create_record_missing_word_id_400 |
| `amas_http.rs` | 2 | process_event_missing_fields_400, process_event_unauthenticated_401 |

**小计: ~12 个测试**

---

## 汇总

| 阶段 | 范围 | 新增测试数 | 预期覆盖提升 |
|------|------|-----------|------------|
| **P0** | Store 操作层 | ~39 | 40% → 50% |
| **P1** | 路由集成测试 | ~51 | 50% → 62% |
| **P2** | Worker 单元测试 | ~31 | 62% → 70% |
| **P3** | AMAS 补充 | ~23 | 70% → 74% |
| **P4** | 属性测试 | ~8 | 74% → 75% |
| **P5** | 边界/错误路径 | ~12 | 75% → 78% |
| **总计** | | **~164** | **40% → 78%** |

## 实施建议

### 测试基础设施增强

1. **扩展 `tests/common/fixtures.rs`**: 添加 `seed_admin`, `seed_wordbook`, `seed_learning_session`, `seed_word_states`, `seed_notifications` 等工厂函数
2. **添加 `tests/common/admin_auth.rs`**: Admin 注册/登录/获取 token 的辅助函数
3. **Worker 测试 helper**: 创建 `src/workers/test_helpers.rs` 提供快速创建带预置数据 Store 的方法

### 执行策略

- P0-P1 可并行开发 (Store 单元测试 + 路由集成测试互不依赖)
- P2 依赖 P0 (Worker 测试需要 Store fixture)
- P3-P5 可在 P0-P2 完成后并行
- 推荐用 4 Agent 团队并行：Store + Routes + Workers + AMAS

### 代码比例目标

- 当前: 874 行测试 / 9,776 行源码 = 0.09
- 目标: ~4,500 行测试 / 9,776 行源码 = 0.46
- 行业参考: 测试/源码比 0.5-1.0 为健康水平
