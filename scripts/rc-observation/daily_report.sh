#!/usr/bin/env bash
# daily_report.sh — 汇总三源采集结果，输出日报 JSON + 判定当日是否全绿（M2-Q4）
#
# 依赖：collect_5xx_rate.sh / collect_sse_incidents.sh / collect_gh_regressions.sh 已跑完，
# 结果文件存于 --obs-dir 下。本脚本也可直接触发三源采集（传入凭证时）。
#
# 用法（汇总已有文件）：
#   ./scripts/rc-observation/daily_report.sh \
#     --day "2026-05-28" \
#     --obs-dir /tmp/rc-obs
#
# 用法（触发采集 + 汇总）：
#   ./scripts/rc-observation/daily_report.sh \
#     --day "2026-05-28" \
#     --obs-dir /tmp/rc-obs \
#     --url "https://your-domain" \
#     --token "$ADMIN_TOKEN" \
#     --gh-token "$GH_TOKEN" \
#     --rc3-date "2026-05-21T00:00:00Z"
#
# 日报输出（同时写 --obs-dir/day-YYYY-MM-DD.json）：
#   {
#     "day": "2026-05-28",
#     "5xx_green": true,   "5xx_rate": 0.000041,
#     "sse_green": true,   "incident_count": 0,
#     "gh_green": true,    "regression_count": 0,
#     "all_green": true,
#     "note": ""
#   }
#
# 退出码：0 = all_green；1 = 不全绿；2 = 脚本自身错误
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DAY=""
OBS_DIR="/tmp/rc-obs"
URL=""
TOKEN=""
GH_TOKEN="${GH_TOKEN:-}"
RC3_DATE=""

usage() {
    echo "用法：$0 --day YYYY-MM-DD --obs-dir <dir> [--url <base_url> --token <admin_token> --gh-token <gh_token> --rc3-date <ISO8601>]"
    exit 2
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --day)      DAY="$2";      shift 2 ;;
        --obs-dir)  OBS_DIR="$2";  shift 2 ;;
        --url)      URL="$2";      shift 2 ;;
        --token)    TOKEN="$2";    shift 2 ;;
        --gh-token) GH_TOKEN="$2"; shift 2 ;;
        --rc3-date) RC3_DATE="$2"; shift 2 ;;
        *) usage ;;
    esac
done

[[ -n "$DAY" ]] || usage
mkdir -p "$OBS_DIR"

# --- 可选：主动触发采集 ---
if [[ -n "$URL" && -n "$TOKEN" ]]; then
    echo "触发 5xx 采集..."
    "${SCRIPT_DIR}/collect_5xx_rate.sh" \
        --url "$URL" --token "$TOKEN" \
        --output "${OBS_DIR}/5xx_${DAY}.json" || true

    echo "触发 SSE incident 采集..."
    "${SCRIPT_DIR}/collect_sse_incidents.sh" \
        --url "$URL" --token "$TOKEN" \
        --output "${OBS_DIR}/incidents.log" \
        --state  "${OBS_DIR}/incident_state.json" || true

    if [[ -n "$RC3_DATE" ]]; then
        echo "触发 GitHub regression 采集..."
        "${SCRIPT_DIR}/collect_gh_regressions.sh" \
            --token "$GH_TOKEN" \
            --since "$RC3_DATE" \
            --output "${OBS_DIR}/regressions_${DAY}.json" || true
    fi
fi

# --- 读取 5xx 结果 ---
FXX_FILE="${OBS_DIR}/5xx_${DAY}.json"
if [[ -f "$FXX_FILE" ]]; then
    FXX_RATE=$(python3 -c "import json; d=json.load(open('${FXX_FILE}')); print(d.get('error_rate', 0))")
    FXX_GREEN=$(python3 -c "import json; d=json.load(open('${FXX_FILE}')); print('true' if d.get('green', False) else 'false')")
else
    echo "WARN: 未找到 5xx 结果文件 ${FXX_FILE}，视为未知（false）" >&2
    FXX_RATE="null"
    FXX_GREEN="false"
fi

# --- 读取 SSE incident 结果 ---
INCIDENT_LOG="${OBS_DIR}/incidents.log"
INCIDENT_COUNT=0
if [[ -f "$INCIDENT_LOG" ]]; then
    # 统计当日新增行数（按 ts 过滤）
    DAY_START=$(date -d "${DAY} 00:00:00" +%s 2>/dev/null || date -j -f "%Y-%m-%d %H:%M:%S" "${DAY} 00:00:00" +%s 2>/dev/null || echo "0")
    DAY_END=$(date -d "${DAY} 23:59:59" +%s 2>/dev/null || date -j -f "%Y-%m-%d %H:%M:%S" "${DAY} 23:59:59" +%s 2>/dev/null || echo "9999999999")
    INCIDENT_COUNT=$(python3 -c "
import json
count = 0
with open('${INCIDENT_LOG}') as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
            ts = d.get('ts', 0)
            if ${DAY_START} <= ts <= ${DAY_END}:
                count += 1
        except Exception:
            pass
print(count)
")
fi
SSE_GREEN=$( [[ "$INCIDENT_COUNT" -eq 0 ]] && echo "true" || echo "false" )

# --- 读取 GitHub regression 结果 ---
GH_FILE="${OBS_DIR}/regressions_${DAY}.json"
GH_COUNT=0
if [[ -f "$GH_FILE" ]]; then
    GH_COUNT=$(python3 -c "import json; d=json.load(open('${GH_FILE}')); print(d.get('count', 0))")
fi
GH_GREEN=$( [[ "$GH_COUNT" -eq 0 ]] && echo "true" || echo "false" )

# --- 最终判定 ---
ALL_GREEN="false"
[[ "$FXX_GREEN" == "true" && "$SSE_GREEN" == "true" && "$GH_GREEN" == "true" ]] && ALL_GREEN="true"

NOTE=""
[[ "$FXX_GREEN"  == "false" ]] && NOTE="${NOTE}5xx_rate=${FXX_RATE}超阈值; "
[[ "$SSE_GREEN"  == "false" ]] && NOTE="${NOTE}incident=${INCIDENT_COUNT}条; "
[[ "$GH_GREEN"   == "false" ]] && NOTE="${NOTE}regression=${GH_COUNT}条; "

REPORT="{\"day\":\"${DAY}\",\"5xx_green\":${FXX_GREEN},\"5xx_rate\":${FXX_RATE:-0},\"sse_green\":${SSE_GREEN},\"incident_count\":${INCIDENT_COUNT},\"gh_green\":${GH_GREEN},\"regression_count\":${GH_COUNT},\"all_green\":${ALL_GREEN},\"note\":\"${NOTE}\"}"

echo "$REPORT" > "${OBS_DIR}/day-${DAY}.json"
echo "日报已写入 ${OBS_DIR}/day-${DAY}.json"
echo "$REPORT"

if [[ "$ALL_GREEN" == "true" ]]; then
    echo "OK: ${DAY} 三源全绿"
else
    echo "ALERT: ${DAY} 不满足稳态门禁：${NOTE}" >&2
    exit 1
fi
