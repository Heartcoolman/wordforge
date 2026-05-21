#!/usr/bin/env bash
# collect_sse_incidents.sh — 轮询 /api/admin/monitoring 获取 incident 告警（M2-Q4 源②）
#
# admin SSE 的 incident 事件由 error_rate_watchdog 推送，payload：
#   { "errorRate": 0.03, "windowSecs": 300 }
# 本脚本改用 REST 轮询 /api/admin/monitoring 接口（SSE 长连接不适合 cron）：
#   通过 /metrics 的 http_5xx 增量判断，新增 incident 写入 incident.log。
#
# 用法：
#   ./scripts/rc-observation/collect_sse_incidents.sh \
#     --url "https://your-domain" \
#     --token "$ADMIN_TOKEN" \
#     --output /tmp/rc-obs/incidents.log \
#     --state  /tmp/rc-obs/incident_state.json
#
# 输出格式（每行一条 JSON）：
#   {"ts":1716300000,"error_rate":0.031,"window_secs":300,"source":"metrics_delta"}
#
# 退出码：0 = 无新 incident；1 = 检测到新 incident；2 = 脚本自身错误
set -euo pipefail

URL=""
TOKEN=""
OUTPUT="/tmp/rc-obs/incidents.log"
STATE_FILE="/tmp/rc-obs/incident_state.json"

usage() {
    echo "用法：$0 --url <base_url> --token <admin_token> [--output <file>] [--state <state.json>]"
    exit 2
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --url)    URL="$2";         shift 2 ;;
        --token)  TOKEN="$2";       shift 2 ;;
        --output) OUTPUT="$2";      shift 2 ;;
        --state)  STATE_FILE="$2";  shift 2 ;;
        *) usage ;;
    esac
done

[[ -n "$URL" && -n "$TOKEN" ]] || usage

mkdir -p "$(dirname "$OUTPUT")" "$(dirname "$STATE_FILE")"

METRICS=$(curl -sf \
    -H "Authorization: Bearer ${TOKEN}" \
    "${URL}/metrics") || {
    echo "ERROR: 无法访问 ${URL}/metrics" >&2
    exit 2
}

extract_counter() {
    local name="$1"
    echo "$METRICS" \
        | grep -E "^${name}(_total)? " \
        | tail -1 \
        | awk '{print $2}' \
        | grep -E '^[0-9]+$' \
        || echo "0"
}

CUR_REQ=$(extract_counter "http_requests")
CUR_5XX=$(extract_counter "http_5xx")
TS=$(date +%s)

# 读上次快照（首次运行时以当前值为基准，不触发告警）
if [[ -f "$STATE_FILE" ]]; then
    PREV_REQ=$(python3 -c "import json,sys; d=json.load(open('${STATE_FILE}')); print(d.get('http_requests',0))")
    PREV_5XX=$(python3 -c "import json,sys; d=json.load(open('${STATE_FILE}')); print(d.get('http_5xx',0))")
else
    # 首次运行：仅保存基准，不判定
    echo "{\"http_requests\":${CUR_REQ},\"http_5xx\":${CUR_5XX},\"ts\":${TS}}" > "$STATE_FILE"
    echo "首次运行，已记录基准快照（http_requests=${CUR_REQ}, http_5xx=${CUR_5XX}）"
    exit 0
fi

# 更新快照
echo "{\"http_requests\":${CUR_REQ},\"http_5xx\":${CUR_5XX},\"ts\":${TS}}" > "$STATE_FILE"

# 计算窗口内增量错误率（同 error_rate_watchdog 逻辑：滚动差值）
RESULT=$(python3 -c "
prev_req = int('${PREV_REQ}')
prev_5xx = int('${PREV_5XX}')
cur_req  = int('${CUR_REQ}')
cur_5xx  = int('${CUR_5XX}')

delta_req = max(0, cur_req - prev_req)
delta_5xx = max(0, cur_5xx - prev_5xx)

if delta_req > 0:
    rate = delta_5xx / delta_req
    # 1% 阈值与后端 error_rate_watchdog THRESHOLD 对齐
    incident = rate > 0.01
else:
    rate = 0.0
    incident = False

import json
print(json.dumps({
    'ts': int('${TS}'),
    'delta_req': delta_req,
    'delta_5xx': delta_5xx,
    'error_rate': round(rate, 6),
    'incident': incident,
    'source': 'metrics_delta'
}))
")

INCIDENT=$(echo "$RESULT" | python3 -c "import json,sys; print(json.load(sys.stdin)['incident'])")

if [[ "$INCIDENT" == "True" ]]; then
    echo "$RESULT" >> "$OUTPUT"
    echo "ALERT: 检测到新 incident（写入 ${OUTPUT}）"
    echo "$RESULT"
    exit 1
fi

echo "OK: 无新 incident（delta_req=$(echo "$RESULT" | python3 -c "import json,sys; print(json.load(sys.stdin)['delta_req'])"), delta_5xx=$(echo "$RESULT" | python3 -c "import json,sys; print(json.load(sys.stdin)['delta_5xx'])")）"
