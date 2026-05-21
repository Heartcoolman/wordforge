#!/usr/bin/env bash
# collect_gh_regressions.sh — 统计 GitHub issue tracker regression label 新增数（M2-Q4 源③）
#
# 用法：
#   ./scripts/rc-observation/collect_gh_regressions.sh \
#     --token "$GH_TOKEN" \
#     --since "2026-05-21T00:00:00Z" \
#     --output /tmp/rc-obs/regressions.json
#
# GH_TOKEN 需要 read:issues scope（public repo 可不带 token，但受 rate limit 60 req/h）。
#
# 输出 JSON：
#   {"ts":1716300000,"since":"2026-05-21T00:00:00Z","count":0,"issues":[],"green":true}
#
# 退出码：0 = 无新增 regression；1 = 有新增；2 = 脚本自身错误
set -euo pipefail

REPO="Heartcoolman/wordforge"
GH_TOKEN="${GH_TOKEN:-}"
SINCE=""
OUTPUT=""

usage() {
    echo "用法：$0 [--token <gh_token>] --since <ISO8601> [--output <file.json>]"
    echo "  --since 示例：2026-05-21T00:00:00Z"
    exit 2
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --token)  GH_TOKEN="$2"; shift 2 ;;
        --since)  SINCE="$2";    shift 2 ;;
        --output) OUTPUT="$2";   shift 2 ;;
        *) usage ;;
    esac
done

[[ -n "$SINCE" ]] || usage

AUTH_HEADER=""
[[ -n "$GH_TOKEN" ]] && AUTH_HEADER="Authorization: Bearer ${GH_TOKEN}"

gh_fetch() {
    local url="$1"
    if [[ -n "$AUTH_HEADER" ]]; then
        curl -sf -H "$AUTH_HEADER" -H "Accept: application/vnd.github+json" "$url"
    else
        curl -sf -H "Accept: application/vnd.github+json" "$url"
    fi
}

API_URL="https://api.github.com/repos/${REPO}/issues"
# labels=regression，state=open，since=ISO8601，按创建时间升序
QUERY="${API_URL}?labels=regression&state=open&since=${SINCE}&per_page=100&sort=created&direction=asc"

RESP=$(gh_fetch "$QUERY") || {
    echo "ERROR: 无法访问 GitHub API（${QUERY}）" >&2
    exit 2
}

TS=$(date +%s)

RESULT=$(python3 -c "
import json, sys

resp = json.loads('''${RESP}''')
issues = []
for item in resp:
    issues.append({
        'number': item['number'],
        'title': item['title'],
        'created_at': item['created_at'],
        'url': item['html_url'],
    })

count = len(issues)
print(json.dumps({
    'ts': ${TS},
    'since': '${SINCE}',
    'count': count,
    'issues': issues,
    'green': count == 0
}, ensure_ascii=False))
") || {
    echo "ERROR: 解析 GitHub API 响应失败" >&2
    exit 2
}

if [[ -n "$OUTPUT" ]]; then
    mkdir -p "$(dirname "$OUTPUT")"
    echo "$RESULT" > "$OUTPUT"
    echo "已写入 ${OUTPUT}"
fi

echo "$RESULT"

COUNT=$(echo "$RESULT" | python3 -c "import json,sys; print(json.load(sys.stdin)['count'])")
if [[ "$COUNT" -gt 0 ]]; then
    echo "ALERT: 发现 ${COUNT} 个新 regression issue（since ${SINCE}）" >&2
    exit 1
fi

echo "OK: 无新增 regression issue（since ${SINCE}）"
