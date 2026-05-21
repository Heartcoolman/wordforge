#!/usr/bin/env bash
# collect_5xx_rate.sh — 采集 /metrics 的 5xx 错误率（M2-Q4 rc.3 稳态观测源①）
#
# 用法：
#   ./scripts/rc-observation/collect_5xx_rate.sh \
#     --url "https://your-domain" \
#     --token "$ADMIN_TOKEN" \
#     --output /tmp/rc-obs/5xx_$(date +%Y%m%d_%H%M).json
#
# 输出 JSON：
#   { "ts": 1716300000, "http_requests": 12345, "http_5xx": 3,
#     "error_rate": 0.000243, "green": true }
#
# 退出码：0 = 全绿（或仅采集）；1 = 错误率超阈值；2 = 脚本自身错误
set -euo pipefail

THRESHOLD="0.001"   # 0.1%
OUTPUT=""
URL=""
TOKEN=""

usage() {
    echo "用法：$0 --url <base_url> --token <admin_token> [--output <file.json>] [--threshold <0.001>]"
    exit 2
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --url)       URL="$2";       shift 2 ;;
        --token)     TOKEN="$2";     shift 2 ;;
        --output)    OUTPUT="$2";    shift 2 ;;
        --threshold) THRESHOLD="$2"; shift 2 ;;
        *) usage ;;
    esac
done

[[ -n "$URL" && -n "$TOKEN" ]] || usage

METRICS=$(curl -sf \
    -H "Authorization: Bearer ${TOKEN}" \
    "${URL}/metrics") || {
    echo "ERROR: 无法访问 ${URL}/metrics" >&2
    exit 2
}

# 从 Prometheus 文本格式提取 counter 值（格式：metric_name_total N 或 metric_name N）
extract_counter() {
    local name="$1"
    echo "$METRICS" \
        | grep -E "^${name}(_total)? " \
        | tail -1 \
        | awk '{print $2}' \
        | grep -E '^[0-9]+$' \
        || echo "0"
}

HTTP_REQUESTS=$(extract_counter "http_requests")
HTTP_5XX=$(extract_counter "http_5xx")
TS=$(date +%s)

# 计算错误率（用 python3 做浮点，避免 bash 整数除法）
RATE=$(python3 -c "
r = int('${HTTP_REQUESTS}')
e = int('${HTTP_5XX}')
print('{:.6f}'.format(e / r if r > 0 else 0.0)
)")

GREEN=$(python3 -c "
print('true' if float('${RATE}') <= float('${THRESHOLD}') else 'false')
")

JSON="{\"ts\":${TS},\"http_requests\":${HTTP_REQUESTS},\"http_5xx\":${HTTP_5XX},\"error_rate\":${RATE},\"threshold\":${THRESHOLD},\"green\":${GREEN}}"

if [[ -n "$OUTPUT" ]]; then
    mkdir -p "$(dirname "$OUTPUT")"
    echo "$JSON" > "$OUTPUT"
    echo "已写入 ${OUTPUT}"
fi

echo "$JSON"

if [[ "$GREEN" == "false" ]]; then
    echo "ALERT: 5xx 错误率 ${RATE} 超过阈值 ${THRESHOLD}" >&2
    exit 1
fi
