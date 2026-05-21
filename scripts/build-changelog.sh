#!/usr/bin/env bash
# 从 GitHub Releases API 拉取全量 release notes，生成 CHANGELOG.md（Keep a Changelog 格式）。
# 用法：
#   GITHUB_TOKEN=ghp_xxx bash scripts/build-changelog.sh
#   bash scripts/build-changelog.sh          # 未设 token 时走 unauthenticated（受 rate limit）
# 后续每次发版手动 append 最新条目即可；此脚本只需在全量重建时重跑。
set -euo pipefail

REPO="Heartcoolman/wordforge"
API_BASE="https://api.github.com/repos/${REPO}"
OUT="CHANGELOG.md"
PER_PAGE=100

AUTH_HEADER=""
if [ -n "${GITHUB_TOKEN:-}" ]; then
  AUTH_HEADER="Authorization: Bearer ${GITHUB_TOKEN}"
fi

fetch() {
  local url="$1"
  if [ -n "$AUTH_HEADER" ]; then
    curl -sf -H "$AUTH_HEADER" -H "Accept: application/vnd.github+json" "$url"
  else
    curl -sf -H "Accept: application/vnd.github+json" "$url"
  fi
}

echo "拉取 releases（最多 ${PER_PAGE} 条）..."
RELEASES=$(fetch "${API_BASE}/releases?per_page=${PER_PAGE}&page=1")

# 写文件头
cat > "$OUT" << 'EOF'
# Changelog

所有版本的变更记录均在此文件，格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

EOF

# 解析 JSON：按 tag_name / published_at / body 依次输出
# 使用 python3（macOS/Linux 均内置）解析，避免 jq 依赖
python3 - "$RELEASES" << 'PYEOF'
import json, sys, re

releases = json.loads(sys.argv[1])
# 按 published_at 降序排列（API 默认已是降序，但显式排保险）
releases.sort(key=lambda r: r.get("published_at", ""), reverse=True)

lines = []
for r in releases:
    tag  = r.get("tag_name", "")
    date = r.get("published_at", "")[:10]  # 取 YYYY-MM-DD
    pre  = r.get("prerelease", False)
    body = (r.get("body") or "").strip()

    pre_label = " \[Pre-release\]" if pre else ""
    lines.append(f"## [{tag}] — {date}{pre_label}\n")
    if body:
        lines.append(body)
    else:
        lines.append("_（无 release notes）_")
    lines.append("\n")

print("\n".join(lines))
PYEOF

echo "已生成 ${OUT}"

# 同步到 docs/changelog.md（VitePress symlink 无法跨 docs/ 外解析，改用实体文件副本）
DOCS_OUT="docs/changelog.md"
if [ -f "$DOCS_OUT" ] || [ -L "$DOCS_OUT" ]; then
  cp "$OUT" "$DOCS_OUT"
  echo "已同步 ${DOCS_OUT}"
fi
