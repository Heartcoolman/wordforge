#!/usr/bin/env bash
# verify-env-example.sh — 校验 .env.example 与 src/config.rs 默认值对齐（M0-P2）
#
# 用法：scripts/verify-env-example.sh
# 退出 0：全部对齐；退出 1：发现不一致，打印差异行。
#
# 仅检测有确定默认值的非密钥项；密钥项（JWT_SECRET 等）用占位符跳过。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_EXAMPLE="${SCRIPT_DIR}/../.env.example"

fail=0

# get_env_value KEY — 从 .env.example 提取 KEY 的值（去除注释行和空行）
get_env_value() {
    local key="$1"
    grep -E "^${key}=" "${ENV_EXAMPLE}" | head -1 | cut -d'=' -f2-
}

# check KEY EXPECTED_VALUE — 如果值不匹配则记录错误
check() {
    local key="$1"
    local expected="$2"
    local actual
    actual="$(get_env_value "${key}")"
    if [[ "${actual}" != "${expected}" ]]; then
        echo "MISMATCH  ${key}: .env.example='${actual}' expected='${expected}'"
        fail=1
    fi
}

# check_present KEY — 只检查该 key 是否存在（不检查具体值，用于密钥占位符）
check_present() {
    local key="$1"
    if ! grep -qE "^${key}=" "${ENV_EXAMPLE}"; then
        echo "MISSING   ${key} 未在 .env.example 中声明"
        fail=1
    fi
}

# --- Runtime ---
check HOST                          "127.0.0.1"
check PORT                          "3000"
check RUST_LOG                      "info"
check API_ONLY                      "false"

# --- Self watchdog ---
check ENABLE_SELF_WATCHDOG          "false"
check SELF_WATCHDOG_INTERVAL_SECS   "15"
check SELF_WATCHDOG_FAILURE_THRESHOLD "3"

# --- Storage ---
check DATABASE_URL                  "./data/learning.db"
check SQLITE_BUSY_TIMEOUT_MS        "5000"
check SQLITE_CONNECTION_TIMEOUT_MS  "250"
check SQLITE_POOL_SIZE              "8"

# --- Auth (仅检查非密钥数值项) ---
check JWT_EXPIRES_IN_HOURS          "24"
check REFRESH_TOKEN_EXPIRES_IN_HOURS "168"
check ADMIN_JWT_EXPIRES_IN_HOURS    "2"
check COOKIE_SECURE                 "false"
check_present JWT_SECRET
check_present ADMIN_JWT_SECRET
check_present REFRESH_JWT_SECRET

# --- Network ---
check CORS_ORIGIN                   "http://localhost:5173"
check TRUST_PROXY                   "false"

# --- Rate limit ---
check RATE_LIMIT_WINDOW_SECS        "900"
check RATE_LIMIT_MAX                "500"
check AUTH_RATE_LIMIT_WINDOW_SECS   "60"
check AUTH_RATE_LIMIT_MAX           "10"
check RATE_LIMIT_TELEMETRY_WINDOW_SECS "60"
check RATE_LIMIT_TELEMETRY_MAX      "120"
check TELEMETRY_RETENTION_DAYS      "90"

# --- Limits ---
check LIMITS_MAX_SSE_CONNECTIONS_PER_USER "5"

# --- Worker ---
check WORKER_LEADER                 "true"
check ENABLE_LLM_ADVISOR_WORKER     "false"
check ENABLE_ENGINE_MONITORING_WORKER "true"

# --- AMAS ---
check AMAS_ENSEMBLE_ENABLED         "true"
check AMAS_MONITOR_SAMPLE_RATE      "0.05"

# --- Logging ---
check ENABLE_FILE_LOGS              "false"
check LOG_DIR                       "./logs"

# --- Pagination ---
check PAGINATION_DEFAULT_SIZE       "20"
check PAGINATION_MAX_SIZE           "100"

# --- Strict mode ---
check STRICT_MODE_ENABLED           "false"
check STRICT_MODE_HARD_BLOCK        "false"
check RECORDS_OUTBOX_ASYNC          "false"

# --- Probe ---
check PROBE_ENABLED                 "false"
check PROBE_RATE_LIMIT_PER_MIN      "10"
check PROBE_MAX_TIMEOUT_MS          "10000"
check PROBE_DEFAULT_TIMEOUT_MS      "3000"
check PROBE_RETENTION_DAYS          "60"

# --- LLM ---
check LLM_ENABLED                   "false"
check LLM_MOCK                      "true"
check LLM_TIMEOUT_SECS              "30"

# --- Update check ---
# 必须使用 list 端点，不能是 /releases/latest（会跳过 prerelease）
check UPDATE_CHECK_API_URL \
    "https://api.github.com/repos/Heartcoolman/wordforge/releases?per_page=10"
check UPDATE_CHECK_CACHE_TTL_SECS   "3600"
check ENABLE_UPDATE_CHECKER_WORKER  "true"
check UPDATE_CHECKER_INTERVAL_SECS  "3600"
check UPDATE_ALLOW_DOWNGRADE        "false"
check UPDATE_MAX_TARBALL_BYTES      "209715200"

if [[ ${fail} -eq 0 ]]; then
    echo "OK  .env.example 与代码默认值完全对齐"
    exit 0
else
    echo ""
    echo "ERROR: 发现上述不一致，请修正 .env.example 或更新本脚本中的期望值"
    exit 1
fi
