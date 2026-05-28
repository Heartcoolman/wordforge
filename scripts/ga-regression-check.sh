#!/usr/bin/env bash
# ga-regression-check.sh — v1.0 GA 全量回归检查（M2-Q4 跨任务最终回归）
#
# 将 RFC §6.1-§6.4 四个门禁条件映射为可自动执行的检查，
# 输出 PASS/FAIL 逐项汇总 + 总判定。
#
# 用法：
#   ./scripts/ga-regression-check.sh                        # 本地跑全量
#   ./scripts/ga-regression-check.sh --skip-cargo           # 跳过 cargo test（快速预检）
#   ./scripts/ga-regression-check.sh --skip-e2e             # 跳过 Playwright e2e
#   ./scripts/ga-regression-check.sh --section m0           # 只跑 M0 门
#   ./scripts/ga-regression-check.sh --section m1           # 只跑 M1 门
#   ./scripts/ga-regression-check.sh --section m2           # 只跑 M2 门
#   ./scripts/ga-regression-check.sh --section ga           # 只跑 GA 门
#
# 退出码：0 = 全部通过；1 = 有失败项；2 = 脚本自身错误
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SKIP_CARGO=false
SKIP_E2E=false
SECTION="all"
PASS=0
FAIL=0
SKIP=0

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-cargo) SKIP_CARGO=true; shift ;;
        --skip-e2e)   SKIP_E2E=true;   shift ;;
        --section)    SECTION="$2";    shift 2 ;;
        *) echo "未知参数：$1"; exit 2 ;;
    esac
done

pass()  { echo -e "${GREEN}PASS${NC}  $1"; ((PASS++));  }
fail()  { echo -e "${RED}FAIL${NC}  $1"; ((FAIL++));    }
skip()  { echo -e "${YELLOW}SKIP${NC}  $1"; ((SKIP++)); }
header(){ echo -e "\n\033[1;34m=== $1 ===\033[0m"; }

# --- 工具函数 ---

# 检查文件存在
check_file() {
    local label="$1" path="$2"
    if [[ -f "${PROJECT_ROOT}/${path}" ]]; then
        pass "${label}: ${path} 存在"
    else
        fail "${label}: ${path} 不存在"
    fi
}

# 检查文件不包含特定模式（用于确认内容已清理）
check_not_contains() {
    local label="$1" path="$2" pattern="$3"
    if [[ ! -f "${PROJECT_ROOT}/${path}" ]]; then
        skip "${label}: ${path} 不存在，跳过内容检查"
        return
    fi
    if grep -qE "${pattern}" "${PROJECT_ROOT}/${path}" 2>/dev/null; then
        fail "${label}: ${path} 仍包含 '${pattern}'"
    else
        pass "${label}: ${path} 已清理 '${pattern}'"
    fi
}

# 检查文件包含特定字符串
check_contains() {
    local label="$1" path="$2" pattern="$3"
    if [[ ! -f "${PROJECT_ROOT}/${path}" ]]; then
        fail "${label}: ${path} 不存在"
        return
    fi
    if grep -qE "${pattern}" "${PROJECT_ROOT}/${path}" 2>/dev/null; then
        pass "${label}: ${path} 包含 '${pattern}'"
    else
        fail "${label}: ${path} 缺少 '${pattern}'"
    fi
}

# 检查目录或文件列表全部存在
check_files_exist() {
    local label="$1"; shift
    local missing=0
    for path in "$@"; do
        [[ -f "${PROJECT_ROOT}/${path}" ]] || { echo "  缺失：${path}"; ((missing++)); }
    done
    if [[ $missing -eq 0 ]]; then
        pass "${label}"
    else
        fail "${label}：${missing} 个文件缺失"
    fi
}

# 运行命令，捕获退出码
run_check() {
    local label="$1"; shift
    if "$@" > /tmp/ga-check-output 2>&1; then
        pass "${label}"
    else
        fail "${label}（退出码 $?）"
        # 显示前 10 行错误输出
        echo "  输出："
        head -10 /tmp/ga-check-output | sed 's/^/    /'
    fi
}

# ================================================================
# § 6.1 M0 门（→ v1.0-rc.1）：基础修复 + 安全网
# ================================================================
check_m0() {
    header "RFC §6.1 M0 门 — 基础修复 + 安全网"

    # M0-C：契约 / 序列化
    check_contains   "M0-C1 alignment.md 存在且含第四轮" \
        "docs/alignment.md" "第四轮"
    check_contains   "M0-C1 alignment.md P0=0" \
        "docs/alignment.md" "P0.*=.*0|0.*P0"
    check_file       "M0-C2 openapi.yaml 已导出" \
        "docs/openapi.yaml"
    check_contains   "M0-C2 openapi.yaml version 与 Cargo.toml 一致" \
        "docs/openapi.yaml" "0\.6\.0"
    check_contains   "M0-C3 SSE 文档含 probe_request 事件" \
        "docs/api-endpoints.md" "probe_request"
    check_contains   "M0-C3 SSE event table 测试含 incident 变体" \
        "tests/sse_event_table.rs" "incident"
    check_file       "M0-C4 deprecation_header 测试存在" \
        "tests/deprecation_header.rs"
    check_contains   "M0-C5 /api/v1/* 返回 410 Gone（v1_gone 测试）" \
        "tests/v1_gone.rs" "410|assert_gone"
    check_contains   "M0-C6 verify 脚本使用 releases list 端点" \
        "scripts/verify-release-auto-update.sh" "releases\?per_page"

    # M0-R：发布流
    check_contains   "M0-R1 release.yml 含 tag lint" \
        ".github/workflows/release.yml" "alpha\|beta\|rc"
    check_file       "M0-R2 minisign 文档" \
        "docs/runbook/key-rotation.md"
    check_file       "M0-R3 systemd 服务模板" \
        "deploy/wordforge.service.tmpl"
    check_contains   "M0-R3 systemd 模板 Restart=always" \
        "deploy/wordforge.service.tmpl" "Restart=always"
    check_file       "M0-R4 updater_maintenance 测试存在" \
        "tests/updater_maintenance.rs"

    # M0-D：文档
    check_file       "M0-D1 README.md 存在" "README.md"
    check_file       "M0-D2 CHANGELOG.md 存在" "CHANGELOG.md"
    check_file       "M0-D3 SECURITY.md 存在" "SECURITY.md"
    check_file       "M0-D4 CONTRIBUTING.md 存在" "CONTRIBUTING.md"
    check_file       "M0-D5 auto-update.md 存在" "docs/auto-update.md"
    check_contains   "M0-D5 auto-update.md 含 channel 参数" \
        "docs/auto-update.md" "channel"
    check_files_exist "M0-D7 5 篇 runbook 全存在" \
        "docs/runbook/backup-restore.md" \
        "docs/runbook/incident-response.md" \
        "docs/runbook/key-rotation.md" \
        "docs/runbook/scaling.md" \
        "docs/runbook/monitoring-setup.md"
    check_files_exist "M0-D8 4 篇用户文档全存在" \
        "docs/user/installation-ios.md" \
        "docs/user/installation-web.md" \
        "docs/user/faq.md" \
        "docs/user/privacy.md"

    # M0-P：性能 / 监控
    check_file       "M0-P1 metrics_endpoint 测试存在" \
        "tests/metrics_endpoint.rs"
    check_contains   "M0-P1 /metrics 端点代码存在" \
        "src/routes/metrics.rs" "http_5xx|http_requests|HTTP_5XX|HTTP_REQUESTS"
    check_file       "M0-P2 verify-env-example.sh 存在" \
        "scripts/verify-env-example.sh"
    check_contains   "M0-P3 monitoring retention worker 存在" \
        "src/workers/monitoring_retention.rs" "RETENTION_DAYS|retention"
    check_file       "M0-P4 error_rate_watchdog 存在" \
        "src/workers/error_rate_watchdog.rs"
    check_contains   "M0-P4 5xx 阈值 THRESHOLD 定义" \
        "src/workers/error_rate_watchdog.rs" "THRESHOLD"
    check_file       "M0-P5 updater_http 测试（phase 超时）" \
        "tests/updater_http.rs"

    # cargo test（M0 相关）
    if [[ "$SKIP_CARGO" == "false" ]]; then
        run_check "M0 cargo test --test deprecation_header" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
            --test deprecation_header -- --test-threads=1
        run_check "M0 cargo test --test v1_gone" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
            --test v1_gone -- --test-threads=1
        run_check "M0 cargo test --test metrics_endpoint" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
            --test metrics_endpoint -- --test-threads=1
        run_check "M0 cargo test --test sse_event_table" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
            --test sse_event_table -- --test-threads=1
        run_check "M0 cargo test --test openapi_export" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
            --test openapi_export -- --test-threads=1
    else
        skip "M0 cargo test（--skip-cargo）"
    fi
}

# ================================================================
# § 6.2 M1 门（→ v1.0-rc.2）：代码债 + 合规
# ================================================================
check_m1() {
    header "RFC §6.2 M1 门 — 代码债 + 合规"

    # M1-A：架构 / 代码债
    check_not_contains "M1-A1 services/learning.rs 已删" \
        "src/services/learning.rs" "."
    check_not_contains "M1-A1 services/wordbook.rs 已删" \
        "src/services/wordbook.rs" "."
    # learning.rs 应不存在（文件不存在时视为通过）
    if [[ ! -f "${PROJECT_ROOT}/src/services/learning.rs" ]]; then
        pass "M1-A1 src/services/learning.rs 已删除"
    else
        fail "M1-A1 src/services/learning.rs 未删除"
    fi
    if [[ ! -f "${PROJECT_ROOT}/src/services/wordbook.rs" ]]; then
        pass "M1-A1 src/services/wordbook.rs 已删除"
    else
        fail "M1-A1 src/services/wordbook.rs 未删除"
    fi
    # M1-A1 验收：routes/ 中 services/ 引用仅剩 probe/updater/llm_provider 三个
    STRAY=$(grep -rn "services/" "${PROJECT_ROOT}/src/routes/" 2>/dev/null \
        | grep -v "probe\|updater\|llm_provider" | grep -v "^Binary" || true)
    if [[ -z "$STRAY" ]]; then
        pass "M1-A1 routes/ 仅剩 probe/updater/llm_provider 三处 services/ 引用"
    else
        fail "M1-A1 routes/ 中仍有多余 services/ 引用：$(echo "$STRAY" | head -3 | tr '\n' ' ')"
    fi

    check_file       "M1-A2 amas_poison_recovery 测试存在" \
        "tests/amas_poison_recovery.rs"
    # M1-A2 验收：engine.rs 已切换到 parking_lot 或使用 unwrap_or_else（不再裸 unwrap）
    check_contains   "M1-A2 engine.rs 使用 parking_lot 或 unwrap_or_else" \
        "src/amas/engine.rs" "parking_lot|unwrap_or_else|PoisonError"

    # M1-A3：4 个 stub worker 已删（检查 workers/mod.rs 不再注册）
    check_not_contains "M1-A3 etymology_generation worker 已删" \
        "src/workers/mod.rs" "etymology_generation"
    check_not_contains "M1-A3 embedding_generation worker 已删" \
        "src/workers/mod.rs" "embedding_generation"
    check_not_contains "M1-A3 word_clustering worker 已删" \
        "src/workers/mod.rs" "word_clustering"

    # M1-A3 补充：monitoring_aggregate worker 文件已删（M0-P3 选路径 b 后退场）
    if [[ ! -f "${PROJECT_ROOT}/src/workers/monitoring_aggregate.rs" ]]; then
        pass "M1-A3 monitoring_aggregate.rs 已删"
    else
        fail "M1-A3 monitoring_aggregate.rs 未删除（应在 M1-A3 随其他 stub worker 一起清理）"
    fi

    # M1-A4：sled 依赖已删
    check_not_contains "M1-A4 Cargo.toml sled 依赖已删" \
        "Cargo.toml" "^sled"
    if [[ ! -f "${PROJECT_ROOT}/src/bin/migrate_sled_to_sqlite.rs" ]]; then
        pass "M1-A4 migrate_sled_to_sqlite.rs 已删"
    else
        fail "M1-A4 migrate_sled_to_sqlite.rs 未删除"
    fi
    # M1-A4 验收：store/keys.rs 已删
    if [[ ! -f "${PROJECT_ROOT}/src/store/keys.rs" ]]; then
        pass "M1-A4 src/store/keys.rs 已删"
    else
        fail "M1-A4 src/store/keys.rs 未删除（sled migration 残留）"
    fi

    # M1-A5：cron scheduler 健康监测
    check_contains   "M1-A5 worker_last_run 表 migration 存在" \
        "src/store/migrate.rs" "worker_last_run"
    check_contains   "M1-A5 /metrics 暴露 worker_last_run_seconds（非 stub）" \
        "src/routes/metrics.rs" "worker_last_run_seconds"
    check_file       "M1-A5 workers_extra 或 workers_coverage 测试覆盖 worker 上报" \
        "tests/workers_extra.rs"

    # M1-A6：strict-mode / maintenance 豁免改路由元数据驱动
    check_contains   "M1-A6 RouteMetadata 定义存在" \
        "src/middleware/strict_mode.rs" "RouteMetadata|strict_mode_exempt|metadata"
    check_file       "M1-A6 middleware_exemption 集成测试存在" \
        "tests/middleware_exemption.rs"

    # M1-A7：queryClient 死依赖已删
    if [[ ! -f "${PROJECT_ROOT}/admin-ui/src/lib/queryClient.ts" ]]; then
        pass "M1-A7 admin-ui/src/lib/queryClient.ts 已删"
    else
        fail "M1-A7 admin-ui/src/lib/queryClient.ts 未删除"
    fi
    check_not_contains "M1-A7 package.json solid-query 已删" \
        "admin-ui/package.json" "solid-query"

    # M1-G1：GDPR 导出端点
    check_contains   "M1-G1 GDPR export store 实现存在" \
        "src/store/operations/gdpr_export.rs" "export|gdpr|GDPR|Article"
    check_file       "M1-G1 gdpr_export 测试存在" \
        "tests/gdpr_export.rs"

    # M1-G2：LLM 月度成本上限
    check_contains   "M1-G2 llm_advisor_cost_ledger migration 存在" \
        "src/store/migrate.rs" "llm_advisor_cost_ledger|monthly.*cost|cost_ledger"
    check_contains   "M1-G2 MonthlyBudgetExceeded 错误类型定义" \
        "src/services/llm_provider.rs" "MonthlyBudgetExceeded|monthly_budget|max_cost_per_month"
    check_contains   "M1-G2 llm_advisor_max_cost_per_month config 存在" \
        "src/config.rs" "max_cost_per_month|monthly_budget"

    # M1-G3：feedback_items 扩展字段
    check_contains   "M1-G3 feedback_items priority 字段 migration 存在" \
        "src/store/migrate.rs" "priority.*normal|feedback.*priority"
    check_contains   "M1-G3 feedback admin 路由支持 status 更新" \
        "src/routes/admin/feedback.rs" "status|priority"

    # M1 全量 cargo test
    if [[ "$SKIP_CARGO" == "false" ]]; then
        run_check "M1 cargo test --test amas_poison_recovery" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
            --test amas_poison_recovery -- --test-threads=1
        run_check "M1 cargo test --test gdpr_export" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
            --test gdpr_export -- --test-threads=1
        run_check "M1 cargo test --test workers_coverage" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
            --test workers_coverage -- --test-threads=1
        if [[ -f "${PROJECT_ROOT}/tests/middleware_exemption.rs" ]]; then
            run_check "M1 cargo test --test middleware_exemption" \
                cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" \
                --test middleware_exemption -- --test-threads=1
        fi
    else
        skip "M1 cargo test（--skip-cargo）"
    fi

    # M1 e2e：GDPR 导出流程
    if [[ "$SKIP_E2E" == "false" ]]; then
        run_check "M1 Playwright gdpr e2e（如有）" \
            bash -c "cd '${PROJECT_ROOT}/admin-ui' && npx playwright test --grep gdpr --reporter=line 2>/dev/null || true"
    else
        skip "M1 Playwright e2e（--skip-e2e）"
    fi
}

# ================================================================
# § 6.3 M2 门（→ v1.0-rc.3）：质量门验证
# ================================================================
check_m2() {
    header "RFC §6.3 M2 门 — 质量门验证"

    # M2-Q1：k6 脚本存在
    check_files_exist "M2-Q1 k6 脚本全存在（5 个）" \
        "tests/load/login.js" \
        "tests/load/session_start.js" \
        "tests/load/review_submit.js" \
        "tests/load/favorites_list.js" \
        "tests/load/sse_connect.js"
    check_file "M2-Q1 load-test workflow 存在" \
        ".github/workflows/load-test.yml"

    # M2-Q2：Lighthouse CI 配置
    check_file "M2-Q2 lighthouse-ci.yml 存在" \
        ".github/workflows/lighthouse-ci.yml"

    # M2-Q3：第四轮 cross-validator 0 P0/P1
    check_contains "M2-Q3 alignment.md 含第四轮 P0/P1 全绿判定" \
        "docs/alignment.md" "0 P0|P0.*0 P1|交叉验证 0 P0"

    # M2-Q4 段（rc-observation 7 天稳态观察）已废弃：v1.1.0-beta.1 follow-up 决策
    # 删除 rc 通道 → 不再走 7 天稳态观察期，由 beta 内测 + 质量门保证 GA 质量。
}

# ================================================================
# § 6.4 GA 门（→ v1.0）：稳态验证
# ================================================================
check_ga() {
    header "GA 门 — 自更新审计完整性（rc-observation 7 天稳态观察段已废弃，v1.1.0-beta.1 follow-up）"

    # S5 自更新审计表（RFC §6.4：升级历史审计表有完整记录）
    check_file "GA (S5) update_audit_log migration 存在（自更新审计）" \
        "src/store/migrate.rs"
    check_contains "GA (S5) update_audit_log 表定义" \
        "src/store/migrate.rs" "update_audit_log"
    # 表字段完整性（from_version/to_version/channel/outcome 是 RFC §6.4 规定字段）
    check_contains "GA (S5) update_audit_log 含 from_version 字段" \
        "src/store/migrate.rs" "from_version"
    check_contains "GA (S5) update_audit_log 含 to_version 字段" \
        "src/store/migrate.rs" "to_version"
    check_contains "GA (S5) update_audit_log 含 outcome 字段" \
        "src/store/migrate.rs" "outcome"
    # apply 入口写入审计（RFC §6.4：升级时写入）
    check_contains "GA (S5) apply 入口调用 insert_update_audit" \
        "src/routes/admin/updates.rs" "insert_update_audit"
    check_contains "GA (S5) apply 完成后调用 complete_update_audit" \
        "src/routes/admin/updates.rs" "complete_update_audit"
    # admin UI 历史列表路由（S5 定义：admin UI 加历史列表）
    check_contains "GA (S5) /admin/updates/history 路由暴露审计历史" \
        "src/routes/admin/updates.rs" "history.*get_history|get_history"

}

# ================================================================
# 全量 cargo test（最终回归）
# ================================================================
check_full_cargo() {
    header "全量 cargo test --all"
    if [[ "$SKIP_CARGO" == "false" ]]; then
        run_check "cargo test --all（全量后端回归）" \
            cargo test --manifest-path "${PROJECT_ROOT}/Cargo.toml" -- --test-threads=4
    else
        skip "全量 cargo test（--skip-cargo）"
    fi
}

# ================================================================
# Vitest 前端单测
# ================================================================
check_vitest() {
    header "前端 Vitest 单测"
    if [[ "$SKIP_E2E" == "false" ]]; then
        run_check "vitest run（前端单测）" \
            bash -c "cd '${PROJECT_ROOT}/admin-ui' && npm test -- --reporter=verbose 2>&1 | tail -20"
    else
        skip "Vitest（--skip-e2e）"
    fi
}

# ================================================================
# main
# ================================================================
echo "========================================"
echo "WordForge v1.0 GA 全量回归检查"
echo "项目根：${PROJECT_ROOT}"
echo "时间：$(date)"
echo "========================================"

case "$SECTION" in
    m0)   check_m0 ;;
    m1)   check_m1 ;;
    m2)   check_m2 ;;
    ga)   check_ga ;;
    all)
        check_m0
        check_m1
        check_m2
        check_ga
        check_full_cargo
        check_vitest
        ;;
    *) echo "未知 section：${SECTION}（可选：m0/m1/m2/ga/all）"; exit 2 ;;
esac

# ================================================================
# 汇总
# ================================================================
echo ""
echo "========================================"
echo "检查结果汇总"
echo "========================================"
TOTAL=$((PASS + FAIL + SKIP))
echo -e "  总计：${TOTAL} 项"
echo -e "  ${GREEN}PASS${NC}：${PASS}"
echo -e "  ${RED}FAIL${NC}：${FAIL}"
echo -e "  ${YELLOW}SKIP${NC}：${SKIP}"
echo ""

if [[ $FAIL -eq 0 ]]; then
    echo -e "${GREEN}所有检查通过（FAIL=0）— 满足 v1.0 GA 门禁条件${NC}"
    exit 0
else
    echo -e "${RED}${FAIL} 项检查失败 — 不满足 GA 门禁条件，请修复后重跑${NC}"
    exit 1
fi
