#!/usr/bin/env bash
# Temporary signing keychain for CI.
#
#   ci-keychain.sh create CERT_VAR PASSWORD_VAR [CERT_VAR PASSWORD_VAR ...]
#   ci-keychain.sh remove
#
# Each CERT_VAR names an environment variable holding a base64 .p12, and
# PASSWORD_VAR the one holding its password. The keychain lives in RUNNER_TEMP,
# goes on the user search list for the job, and `remove` takes it off again.
# `create` records the path in GITHUB_ENV as SIGNING_KEYCHAIN.
set -euo pipefail

: "${RUNNER_TEMP:?RUNNER_TEMP is not set; this script is for GitHub Actions}"
KEYCHAIN="${RUNNER_TEMP}/onde-signing.keychain-db"

create() {
  (( $# >= 2 && $# % 2 == 0 )) || { echo "usage: $0 create CERT_VAR PASSWORD_VAR [...]" >&2; exit 1; }
  local pass; pass="$(openssl rand -base64 24)"
  security create-keychain -p "$pass" "$KEYCHAIN"
  security set-keychain-settings -lut 21600 "$KEYCHAIN"
  security unlock-keychain -p "$pass" "$KEYCHAIN"

  while (( $# )); do
    local cert_var="$1" pass_var="$2"; shift 2
    [[ -n "${!cert_var:-}" ]] || { echo "::error::${cert_var} is empty. Is the secret set?"; exit 1; }
    local p12="${RUNNER_TEMP}/${cert_var}.p12"
    printf '%s' "${!cert_var}" | base64 --decode > "$p12"
    security import "$p12" -k "$KEYCHAIN" -P "${!pass_var:-}" \
      -T /usr/bin/codesign -T /usr/bin/productbuild -T /usr/bin/security
    rm -f "$p12"
  done

  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pass" "$KEYCHAIN" >/dev/null

  local existing=()
  while IFS= read -r entry; do
    entry="${entry#"${entry%%[![:space:]]*}"}"; entry="${entry%\"}"; entry="${entry#\"}"
    [[ -n "$entry" ]] && existing+=("$entry")
  done < <(security list-keychains -d user)
  security list-keychains -d user -s "$KEYCHAIN" "${existing[@]}"
  echo "SIGNING_KEYCHAIN=${KEYCHAIN}" >> "${GITHUB_ENV:?}"
  security find-identity -v "$KEYCHAIN"
}

remove() {
  [[ -e "$KEYCHAIN" ]] || return 0
  local remaining=()
  while IFS= read -r entry; do
    entry="${entry#"${entry%%[![:space:]]*}"}"; entry="${entry%\"}"; entry="${entry#\"}"
    [[ -z "$entry" || "$entry" == "$KEYCHAIN" || "$entry" == "/private${KEYCHAIN}" ]] && continue
    remaining+=("$entry")
  done < <(security list-keychains -d user)
  (( ${#remaining[@]} )) && security list-keychains -d user -s "${remaining[@]}"
  security delete-keychain "$KEYCHAIN" || true
}

case "${1:-}" in
  create) shift; create "$@" ;;
  remove) remove ;;
  *) echo "usage: $0 create CERT_VAR PASSWORD_VAR [...] | remove" >&2; exit 1 ;;
esac
