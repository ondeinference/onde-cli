#!/usr/bin/env bash
# Shared by build-appstore.sh and build-direct.sh. Source it, do not run it.
#
# Everything here is overridable from the environment, and nothing secret has a
# default: the App Store Connect key id and issuer come from ASC_API_KEY and
# ASC_ISSUER_ID, and the .p8 from ~/.appstoreconnect/private_keys or
# API_PRIVATE_KEYS_DIR (CI points that at RUNNER_TEMP).

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLI_ROOT="$(cd "${APP_DIR}/../.." && pwd)"
# shellcheck disable=SC2034  # used by the scripts that source this file
TAURI_DIR="${APP_DIR}/src-tauri"
APP_NAME="${APP_NAME:-Onde Desktop}"

step() { echo ""; echo "── $* ──────────────────────────────────────────────"; }
ok()   { echo "✅  $*"; }
fail() { printf "❌  %b\n" "$*" >&2; exit 1; }

# mlx-sys builds MLX with CMake, which takes the deployment target from this
# variable (otherwise from rustc's default for aarch64, 11.0) and refuses
# anything below 14. It matches minimumSystemVersion in the Tauri configs.
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"

# The Rust build bakes the Onde credentials in with env!(). Locally they live in
# the repo-root .env; CI exports them straight from secrets.
load_dotenv() {
  if [[ -f "${CLI_ROOT}/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "${CLI_ROOT}/.env"
    set +a
  fi
  local v
  for v in ONDE_APP_ID ONDE_APP_SECRET HF_TOKEN GRESIQ_API_KEY GRESIQ_API_SECRET; do
    [[ -n "${!v:-}" ]] || fail "${v} is not set.\n       Put it in ${CLI_ROOT}/.env or export it."
  done
}

# A beta Xcode corrupts proc-macro dylibs when cross-compiling, and App Store
# Connect rejects binaries built with a beta SDK. Pick a release one.
pick_release_xcode() {
  local active app
  active="$(xcode-select -p 2>/dev/null || true)"
  if [[ -n "$active" && "$active" != *[Bb]eta* ]]; then
    printf '%s\n' "$active"; return
  fi
  for app in /Applications/Xcode*.app; do
    case "$app" in *[Bb]eta*) continue ;; esac
    [[ -d "$app/Contents/Developer" ]] && { printf '%s\n' "$app/Contents/Developer"; return; }
  done
}

require_release_xcode() {
  DEVELOPER_DIR="${DEVELOPER_DIR:-$(pick_release_xcode)}"
  export DEVELOPER_DIR
  [[ -n "${DEVELOPER_DIR}" && -d "${DEVELOPER_DIR}" ]] \
    || fail "No release Xcode found.\n       Install one, or set DEVELOPER_DIR to its Contents/Developer dir."
  case "${DEVELOPER_DIR}" in
    *[Bb]eta*) fail "DEVELOPER_DIR points at a beta Xcode:\n       ${DEVELOPER_DIR}" ;;
  esac
  echo "🛠   Xcode  → $(xcodebuild -version 2>/dev/null | head -1)  [${DEVELOPER_DIR}]"
}

# CFBundleVersion: YYYYMMDD.HHMM (UTC). Two integers, increasing with time.
bundle_version() { date -u +"%Y%m%d.%H%M"; }

# The same number goes into CFBundleVersion (through the Tauri config) and into
# the binary (build.rs reads ONDE_DESKTOP_BUILD), so the sidebar can show it.
export_build_number() { export ONDE_DESKTOP_BUILD="$1"; }

# MLX loads its Metal kernels from mlx.metallib next to the app binary. Without
# the file, training quietly falls back to the CPU, which is many times slower.
# mlx-sys builds it under the cargo target directory, so compile for Apple
# Silicon first and hand the file's path to the bundle.
prebuild_mlx() {
  step "MLX kernels"
  (cd "$TAURI_DIR" && cargo build --release --target aarch64-apple-darwin --features tauri/custom-protocol)
}

# Absolute path of the mlx.metallib that prebuild_mlx left behind.
mlx_metallib() {
  (cd "$TAURI_DIR" && "${CLI_ROOT}/packaging/macos/find-mlx-metallib.sh" aarch64-apple-darwin "${1:-release}")
}

# Fail unless the built .app carries the kernels next to its binary.
require_metallib() {
  [[ -f "$1/Contents/MacOS/mlx.metallib" ]] \
    || fail "mlx.metallib is not in $1/Contents/MacOS. Training would fall back to the CPU."
}

# JSON for one extra --config. node is already needed for the frontend, and it
# quotes paths correctly. The third argument is the mlx.metallib to bundle.
config_json() {
  BUNDLE_VERSION="$1" PROFILE="$2" METALLIB="$3" node -e '
    const m = { bundleVersion: process.env.BUNDLE_VERSION,
                files: { "embedded.provisionprofile": process.env.PROFILE,
                         "MacOS/mlx.metallib": process.env.METALLIB } };
    process.stdout.write(JSON.stringify({ bundle: { macOS: m } }));'
}

require_profile() {
  local p="$1" label="$2"
  [[ -f "$p" ]] || fail "${label} provisioning profile not found: ${p}\n       Set the path in the environment, or download it with: asc profiles download"
}

# Leaf cert SHA-1 of a signed .app, uppercase without colons. codesign writes the
# chain to the current directory, so run it in a scratch one.
codesign_leaf_sha1() {
  local app="$1" tmp
  tmp="$(mktemp -d -t codesign-leaf-XXXXXX)"
  (
    cd "$tmp" || exit 1
    codesign -d --extract-certificates "$app" >/dev/null 2>&1
    [[ -s codesign0 ]] || exit 1
    openssl x509 -inform DER -in codesign0 -fingerprint -sha1 -noout \
      | awk -F'=' '{gsub(":","",$2); print toupper($2)}'
  )
  local rc=$?
  rm -rf "$tmp"
  return $rc
}

# SHA-1 of DeveloperCertificates[0] in a .provisionprofile (CMS-wrapped plist).
profile_leaf_sha1() {
  security cms -D -i "$1" 2>/dev/null \
    | plutil -extract DeveloperCertificates.0 raw -o - - 2>/dev/null \
    | base64 --decode \
    | openssl x509 -inform DER -fingerprint -sha1 -noout \
    | awk -F'=' '{gsub(":","",$2); print toupper($2)}'
}

verify_codesign_matches_profile() {
  local app="$1" prof="${1}/Contents/embedded.provisionprofile" app_sha prof_sha
  [[ -f "$prof" ]] || fail "embedded.provisionprofile is not inside the .app: ${prof}"
  app_sha="$(codesign_leaf_sha1 "$app")"  || fail "Could not read the codesign cert from ${app}."
  prof_sha="$(profile_leaf_sha1 "$prof")" || fail "Could not read DeveloperCertificates[0] from ${prof}."
  echo "🔏  codesign leaf → ${app_sha}"
  echo "📜  profile leaf  → ${prof_sha}"
  [[ "$app_sha" == "$prof_sha" ]] \
    || fail "The app is signed with a cert the profile does not allow.\n       Re-pin signingIdentity in the Tauri config, or regenerate the profile."
  ok "Codesign cert matches the provisioning profile"
}

# Fail unless the signed app carries every given entitlement key as true or present.
require_entitlements() {
  local app="$1"; shift
  local ents key
  ents="$(codesign -d --entitlements - --xml "$app" 2>/dev/null)" || fail "Could not read entitlements of ${app}."
  for key in "$@"; do
    grep -q "<key>${key}</key>" <<<"$ents" || fail "Entitlement ${key} is missing from the signed app."
  done
  ok "Entitlements present: $*"
}

require_asc_credentials() {
  [[ -n "${ASC_API_KEY:-}" ]]   || fail "ASC_API_KEY (App Store Connect key id) is not set."
  [[ -n "${ASC_ISSUER_ID:-}" ]] || fail "ASC_ISSUER_ID (App Store Connect issuer) is not set."
  local dir="${API_PRIVATE_KEYS_DIR:-${HOME}/.appstoreconnect/private_keys}"
  ASC_KEY_PATH="${APPLE_API_KEY_PATH:-${dir}/AuthKey_${ASC_API_KEY}.p8}"
  [[ -f "$ASC_KEY_PATH" ]] || fail "App Store Connect key not found: ${ASC_KEY_PATH}"
}
