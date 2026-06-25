#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAK="${NAK:-/Users/pablofernandez/go/bin/nak}"
SIMULATOR_ID="${SIMULATOR_ID:-A45FE049-A23E-4949-85C7-58BD2322B868}"
PORT="${M002_UAT_PORT:-$((20000 + RANDOM % 20000))}"
HOST="127.0.0.1"
RELAY_URL="ws://${HOST}:${PORT}"
TMP_DIR="$(mktemp -d)"
SEED_FILE="${TMP_DIR}/m002-seed.jsonl"
PROJECTION_FILE="${TMP_DIR}/m002-projections.json"
CONFIG_FILE="/tmp/29er-m002-uat-env.json"

cleanup() {
  local status=$?
  if [[ "${status}" -ne 0 && -f "${TMP_DIR}/relay.log" ]]; then
    printf '\n--- nak relay log ---\n' >&2
    cat "${TMP_DIR}/relay.log" >&2 || true
    printf '%s\n' '--- end nak relay log ---' >&2
  fi
  if [[ -n "${RELAY_PID:-}" ]]; then
    kill "${RELAY_PID}" 2>/dev/null || true
    wait "${RELAY_PID}" 2>/dev/null || true
  fi
  rm -f "${CONFIG_FILE}"
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

emit_event() {
  "${NAK}" event --sec "${APP_SECRET_HEX}" "$@" >> "${SEED_FILE}"
}

event_json() {
  "${NAK}" event --sec "${APP_SECRET_HEX}" "$@"
}

emit_group() {
  local id="$1"
  local name="$2"
  local member_pubkey="${3:-}"
  local admin_pubkey="${4:-}"

  emit_event -k 39000 -d "${id}" -t "name=${name}" -t public -t open -c ''
  if [[ -n "${admin_pubkey}" ]]; then
    emit_event -k 39001 -d "${id}" -p "${admin_pubkey}" -c ''
  fi
  if [[ -n "${member_pubkey}" ]]; then
    emit_event -k 39002 -d "${id}" -p "${member_pubkey}" -c ''
  else
    emit_event -k 39002 -d "${id}" -c ''
  fi
}

APP_SECRET_HEX="$("${NAK}" key generate)"
APP_NSEC="$("${NAK}" encode nsec "${APP_SECRET_HEX}")"
APP_PUBKEY="$("${NAK}" key public "${APP_SECRET_HEX}")"
TARGET_SECRET_HEX="$("${NAK}" key generate)"
TARGET_PUBKEY="$("${NAK}" key public "${TARGET_SECRET_HEX}")"
SEED_TS="$(date +%s)"
PROJECTION_TS="$((SEED_TS + 20))"

emit_group "m002-admin-root" "M002 Admin Root" "${APP_PUBKEY}" "${APP_PUBKEY}"
emit_group "m002-joinable" "M002 Joinable" "" ""
emit_group "m002-leavable" "M002 Leavable" "${APP_PUBKEY}" ""
emit_group "m002-alt-parent" "M002 Alt Parent" "${APP_PUBKEY}" "${APP_PUBKEY}"
emit_group "m002-movable" "M002 Movable" "${APP_PUBKEY}" "${APP_PUBKEY}"

JOINED_EVENT="$(event_json --created-at "$((PROJECTION_TS + 1))" -k 39002 -d m002-joinable -p "${APP_PUBKEY}" -c '')"
LEFT_EVENT="$(event_json --created-at "$((PROJECTION_TS + 2))" -k 39002 -d m002-leavable -c '')"
ADMIN_MEMBERS_EVENT="$(event_json --created-at "$((PROJECTION_TS + 3))" -k 39002 -d m002-admin-root -p "${APP_PUBKEY}" -p "${TARGET_PUBKEY}" -c '')"
ADMIN_ROOT_CHILD_EVENT="$(event_json --created-at "$((PROJECTION_TS + 4))" -k 39000 -d m002-admin-root -t "name=M002 Admin Root" -t public -t open -t child=m002-child-ui -c '')"
CHILD_EVENT="$(event_json --created-at "$((PROJECTION_TS + 5))" -k 39000 -d m002-child-ui -t "name=M002 Child UI" -t "about=created by M002 UI UAT" -t public -t open -t parent=m002-admin-root -c '')"
ALT_PARENT_CHILD_EVENT="$(event_json --created-at "$((PROJECTION_TS + 6))" -k 39000 -d m002-alt-parent -t "name=M002 Alt Parent" -t public -t open -t child=m002-movable -c '')"
MOVED_EVENT="$(event_json --created-at "$((PROJECTION_TS + 7))" -k 39000 -d m002-movable -t "name=M002 Movable" -t public -t open -t parent=m002-alt-parent -c '')"

jq -n \
  --argjson joined "${JOINED_EVENT}" \
  --argjson left "${LEFT_EVENT}" \
  --argjson adminMembers "${ADMIN_MEMBERS_EVENT}" \
  --argjson adminRootChild "${ADMIN_ROOT_CHILD_EVENT}" \
  --argjson child "${CHILD_EVENT}" \
  --argjson altParentChild "${ALT_PARENT_CHILD_EVENT}" \
  --argjson moved "${MOVED_EVENT}" \
  '{
    joined: $joined,
    left: $left,
    adminMembers: $adminMembers,
    adminRootChild: $adminRootChild,
    child: $child,
    altParentChild: $altParentChild,
    moved: $moved
  }' > "${PROJECTION_FILE}"

jq -s \
  --arg relayURL "${RELAY_URL}" \
  --arg nsec "${APP_NSEC}" \
  --arg targetPubkey "${TARGET_PUBKEY}" \
  --slurpfile projectionEvents "${PROJECTION_FILE}" \
  '{relayURL: $relayURL, nsec: $nsec, targetPubkey: $targetPubkey, seedEvents: ., projectionEvents: $projectionEvents[0]}' \
  "${SEED_FILE}" > "${CONFIG_FILE}"

"${NAK}" serve --hostname "${HOST}" --port "${PORT}" >"${TMP_DIR}/relay.log" 2>&1 &
RELAY_PID="$!"

ready=0
for _ in {1..25}; do
  if "${NAK}" req -k 39000 -d m002-joinable -l 1 "${RELAY_URL}" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.2
done
if [[ "${ready}" -ne 1 ]]; then
  echo "M002 UAT relay did not become ready at ${RELAY_URL}" >&2
  exit 1
fi

xcrun simctl uninstall "${SIMULATOR_ID}" io.f7z.app29er >/dev/null 2>&1 || true

cd "${ROOT_DIR}/ios/29er"
xcodebuild test \
  -project 29er.xcodeproj \
  -scheme 29er \
  -destination "id=${SIMULATOR_ID}" \
  -only-testing:29erUITests/M002RelayFlowUITests/testM002RelayActionsPublishToRelay
