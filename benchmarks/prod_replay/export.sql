-- benchmarks/prod_replay/export.sql
-- Production-log replay audit: read-only export queries Q0-Q4.
-- Spec: docs/amas-tuning-2026-06-13-hardening campaign, W3 (prod-replay design spec §5).
-- Run on the prod host with `sqlite3 -readonly "$DB" ...` (DB path under /opt/wordforge,
-- exact filename confirmed on host). NOTHING here writes.

-- ============================================================================
-- Q0 — preflight volumes (run first)
-- ============================================================================
SELECT COUNT(*), COUNT(DISTINCT user_id), MIN(created_at), MAX(created_at) FROM learning_records;
SELECT COUNT(*) FROM engine_algo_states WHERE algo_id LIKE 'mastery:%';
SELECT COUNT(*) FROM mastery_states;  -- expect 0/stale (write-orphaned, spec §1.2)
SELECT COUNT(*) FROM users;

-- ============================================================================
-- Q1 — events (replay input)
-- Invocation:
--   sqlite3 -readonly -csv -header "$DB" \
--    "<query below>" > prod_events.csv
-- ============================================================================
SELECT user_id, word_id, created_at, is_correct, response_time_ms, record_type, session_id
FROM learning_records
ORDER BY user_id, word_id, created_at, id;

-- ============================================================================
-- Q2 — live memory states (blast-radius input)
-- Invocation:
--   sqlite3 -readonly -csv -header "$DB" \
--    "<query below>" > prod_states.csv
-- (substr(algo_id,9) strips the 8-char 'mastery:' prefix; SQLite substr is 1-indexed.)
-- ============================================================================
SELECT user_id,
       substr(algo_id, 9)                                   AS word_id,
       json_extract(state_json,'$.mdm.stability')           AS stability,
       json_extract(state_json,'$.mdm.difficulty')          AS difficulty,
       json_extract(state_json,'$.mdm.review_count')        AS review_count,
       json_extract(state_json,'$.mdm.last_review_at')      AS last_review_at_ms,
       json_extract(state_json,'$.correct_streak')          AS correct_streak,
       json_extract(state_json,'$.total_attempts')          AS total_attempts,
       json_extract(state_json,'$.total_correct')           AS total_correct
FROM engine_algo_states
WHERE algo_id LIKE 'mastery:%';

-- ============================================================================
-- Q3 — config verification (which OLD semantics actually ran)
-- Invocation:
--   sqlite3 -readonly -json "$DB" \
--    "<query below>" > prod_config_versions.json
--   cat /opt/wordforge/amas_config.toml > prod_amas_config.toml
--   (file is authoritative at startup, src/main.rs:150-176)
-- ============================================================================
SELECT version_hash, source, created_at, note, snapshot_json
FROM amas_config_versions ORDER BY created_at DESC LIMIT 3;

-- ============================================================================
-- Q4 — cross-check (optional): sanity-check tool-computed old intervals
-- against the served next_review_date.
-- ============================================================================
SELECT user_id, word_id, next_review_date, correct_streak, half_life FROM word_learning_states;
