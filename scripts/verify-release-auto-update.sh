#!/usr/bin/env bash
set -euo pipefail

OLD_TAG="${OLD_TAG:-v0.4.3}"
NEW_TAG="${NEW_TAG:-v0.4.4}"
# M0-C6：双通道升级时指定通道；stable 为默认，beta 测试时设 CHANNEL=beta
CHANNEL="${CHANNEL:-stable}"
ARCH="${ARCH:-x86_64}"
PORT="${PORT:-31080}"
BASE="http://127.0.0.1:${PORT}"
WORKDIR="$(mktemp -d)"
INSTALL_DIR="${WORKDIR}/install"
ADMIN_EMAIL="verify-auto-update@example.com"
ADMIN_PASSWORD="VerifyAutoUpdate123!"

cleanup() {
  if [[ -f "${WORKDIR}/wordforge.pid" ]]; then
    kill "$(cat "${WORKDIR}/wordforge.pid")" >/dev/null 2>&1 || true
  fi
  rm -rf "${WORKDIR}"
}
trap cleanup EXIT

dump_debug() {
  echo "---- process status ----" >&2
  if [[ -f "${WORKDIR}/wordforge.pid" ]]; then
    ps -fp "$(cat "${WORKDIR}/wordforge.pid")" >&2 || true
  fi
  echo "---- install dir ----" >&2
  find "${INSTALL_DIR:-${WORKDIR}}" -maxdepth 2 -type f -o -type d 2>/dev/null | sort >&2 || true
  echo "---- server log ----" >&2
  if [[ -f "${WORKDIR}/server.log" ]]; then
    tail -240 "${WORKDIR}/server.log" >&2 || true
  fi
}

wait_for_status_version() {
  local expected="$1"
  local deadline=$((SECONDS + 120))
  local body
  while (( SECONDS < deadline )); do
    body="$(curl -fsS "${BASE}/api/status" 2>/dev/null || true)"
    if echo "${body}" | jq -e --arg v "${expected}" '.data.version == $v' >/dev/null 2>&1; then
      echo "${body}"
      return 0
    fi
    sleep 2
  done
  echo "Timed out waiting for /api/status version ${expected}" >&2
  dump_debug
  return 1
}

download_release_asset() {
  local tag="$1"
  local asset="wordforge-linux-${ARCH}.tar.gz"
  gh release download "${tag}" \
    --repo Heartcoolman/wordforge \
    --pattern "${asset}" \
    --dir "${WORKDIR}/${tag}"
  mkdir -p "${INSTALL_DIR}"
  tar -xzf "${WORKDIR}/${tag}/${asset}" -C "${WORKDIR}/${tag}"
  cp -R "${WORKDIR}/${tag}/wordforge-linux-${ARCH}/." "${INSTALL_DIR}/"
}

download_release_asset "${OLD_TAG}"
chmod +x "${INSTALL_DIR}/wordforge"
mkdir -p "${INSTALL_DIR}/data" "${INSTALL_DIR}/logs"

cat > "${INSTALL_DIR}/.env" <<EOF
HOST=127.0.0.1
PORT=${PORT}
DATABASE_URL=${INSTALL_DIR}/data/learning.db
JWT_SECRET=verify_jwt_secret_0123456789_0123456789_0123456789
REFRESH_JWT_SECRET=verify_refresh_secret_0123456789_0123456789_0123456789
ADMIN_JWT_SECRET=verify_admin_secret_0123456789_0123456789_0123456789
RUST_LOG=info
ENABLE_FILE_LOGS=true
LOG_DIR=${INSTALL_DIR}/logs
API_ONLY=false
LLM_ENABLED=false
LLM_MOCK=true
# M0-C6：beta.3 双通道后后端需要列表端点分流 stable/beta；禁止 hardcode /tags/...
UPDATE_CHECK_API_URL=https://api.github.com/repos/Heartcoolman/wordforge/releases?per_page=10
ENABLE_UPDATE_CHECKER_WORKER=true
UPDATE_CHECK_CACHE_TTL_SECS=3600
UPDATE_INSTALL_DIR=${INSTALL_DIR}
UPDATE_ALLOW_DOWNGRADE=false
UPDATE_MAX_TARBALL_BYTES=209715200
WORDFORGE_GITHUB_TOKEN=${GITHUB_TOKEN:-}
EOF

(
  cd "${INSTALL_DIR}"
  ./wordforge
) >"${WORKDIR}/server.log" 2>&1 &
echo "$!" > "${WORKDIR}/wordforge.pid"

old_status="$(wait_for_status_version "${OLD_TAG}")"
echo "Initial status: ${old_status}"

setup_response="$(curl -fsS -X POST "${BASE}/api/admin/auth/setup" \
  -H 'Content-Type: application/json' \
  -d "{\"email\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASSWORD}\"}")"
token="$(echo "${setup_response}" | jq -r '.data.token')"
if [[ -z "${token}" || "${token}" == "null" ]]; then
  echo "Failed to create admin and read token" >&2
  echo "${setup_response}" >&2
  exit 1
fi
echo "Admin account created for verification."

status_before="$(curl -fsS -H "Authorization: Bearer ${token}" "${BASE}/api/admin/updates/status")"
echo "Updater cached status before check: ${status_before}"
echo "${status_before}" | jq -e \
  --arg current "${OLD_TAG}" \
  '.data.currentVersion == $current and .data.autoCheckEnabled == true' >/dev/null

check_response="$(curl -fsS -X POST -H "Authorization: Bearer ${token}" "${BASE}/api/admin/updates/check")"
echo "Manual check response: ${check_response}"
# M0-C6：beta.3 双通道后 check 响应 latestVersion/hasUpdate/canApply 移入 .data.stable/.data.beta 子对象
echo "${check_response}" | jq -e \
  --arg current "${OLD_TAG}" \
  --arg latest "${NEW_TAG}" \
  --arg ch "${CHANNEL}" \
  '.data.currentVersion == $current and .data[$ch].latestVersion == $latest and .data[$ch].hasUpdate == true and .data[$ch].canApply == true' >/dev/null

set +e
# M0-C6：beta.3 新增 channel 必填字段（"stable" | "beta"）
apply_response="$(curl -sS -m 20 -X POST "${BASE}/api/admin/updates/apply" \
  -H "Authorization: Bearer ${token}" \
  -H 'Content-Type: application/json' \
  -d "{\"targetVersion\":\"${NEW_TAG}\",\"confirmCurrentVersion\":\"${OLD_TAG}\",\"channel\":\"${CHANNEL}\"}" 2>&1)"
apply_code=$?
set -e
echo "Apply curl exit code: ${apply_code}"
echo "Apply response: ${apply_response}"

new_status="$(wait_for_status_version "${NEW_TAG}")"
echo "Final status: ${new_status}"
echo "${new_status}" | jq -e --arg v "${NEW_TAG}" '.data.version == $v' >/dev/null

test -x "${INSTALL_DIR}/wordforge.${OLD_TAG}"
test -d "${INSTALL_DIR}/static.${OLD_TAG}"
test -f "${INSTALL_DIR}/data/learning-${OLD_TAG}.backup.db"

post_update_status="$(curl -fsS -H "Authorization: Bearer ${token}" "${BASE}/api/admin/updates/status")"
echo "Updater status after restart: ${post_update_status}"
echo "${post_update_status}" | jq -e --arg current "${NEW_TAG}" '.data.currentVersion == $current' >/dev/null

echo "Auto update verification passed: ${OLD_TAG} -> ${NEW_TAG}."
