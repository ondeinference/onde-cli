#!/usr/bin/env bash
# Direct (Developer ID) distribution of Onde Desktop: a signed, notarized,
# stapled DMG.
#   build   tauri build (universal, Developer ID config). Tauri signs the app and
#           notarizes and staples it; the script then does the same for the DMG
#   verify  spctl, stapler and codesign checks on the result
#
# Usage: scripts/build-direct.sh [build|verify|all]
#
# Environment (all optional unless noted):
#   ASC_API_KEY, ASC_ISSUER_ID   required; the .p8 is read from API_PRIVATE_KEYS_DIR
#                                or ~/.appstoreconnect/private_keys
#   DEVID_PROVISIONING_PROFILE_PATH  default: the private apple-certificates checkout
#   DEVELOPER_DIR, BUNDLE_VERSION    as in build-appstore.sh
set -euo pipefail
# shellcheck source=release-lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/release-lib.sh"

CONFIG="${TAURI_DIR}/tauri.prod-macos.conf.json"
DEVID_PROVISIONING_PROFILE_PATH="${DEVID_PROVISIONING_PROFILE_PATH:-${HOME}/Repositories/apple-certificates/onde-desktop/OndeDesktop_DeveloperID.provisionprofile}"
TARGET_DIR="${CARGO_TARGET_DIR:-${TAURI_DIR}/target}"
[[ "$TARGET_DIR" = /* ]] || TARGET_DIR="${APP_DIR}/${TARGET_DIR}"
BUNDLE_DIR="${TARGET_DIR}/universal-apple-darwin/release/bundle"
APP_PATH="${BUNDLE_DIR}/macos/${APP_NAME}.app"

find_dmg() { find "${BUNDLE_DIR}/dmg" -maxdepth 1 -name '*.dmg' -print -quit 2>/dev/null; }

do_build() {
  step "1/2  Build, sign, notarize"
  require_release_xcode
  load_dotenv
  require_profile "$DEVID_PROVISIONING_PROFILE_PATH" "Developer ID"
  require_asc_credentials

  # Tauri notarizes with notarytool and staples when these three are set.
  export APPLE_API_KEY="$ASC_API_KEY"
  export APPLE_API_ISSUER="$ASC_ISSUER_ID"
  export APPLE_API_KEY_PATH="$ASC_KEY_PATH"

  BUNDLE_VERSION="${BUNDLE_VERSION:-$(bundle_version)}"
  export_build_number "$BUNDLE_VERSION"
  echo "🏷   bundleVersion → ${BUNDLE_VERSION}"
  echo "📜  profile       → ${DEVID_PROVISIONING_PROFILE_PATH}"

  (cd "$APP_DIR" && pnpm tauri build \
    --bundles app,dmg \
    --target universal-apple-darwin \
    --config "$CONFIG" \
    --config "$(config_json "$BUNDLE_VERSION" "$DEVID_PROVISIONING_PROFILE_PATH")")

  [[ -d "$APP_PATH" ]] || fail "Build succeeded but the .app is not at: ${APP_PATH}"
  [[ -n "$(find_dmg)" ]] || fail "Build succeeded but there is no .dmg in ${BUNDLE_DIR}/dmg"
  notarize_dmg "$(find_dmg)"
  ok "Build complete → $(find_dmg)"
}

# Tauri notarizes and staples the .app but leaves the DMG alone, and Gatekeeper
# assesses the DMG as a download. Notarize and staple it as well.
notarize_dmg() {
  local dmg="$1"
  step "1.5  Notarize the DMG"
  xcrun notarytool submit "$dmg" --key "$ASC_KEY_PATH" --key-id "$ASC_API_KEY" \
    --issuer "$ASC_ISSUER_ID" --wait
  xcrun stapler staple "$dmg"
}

do_verify() {
  step "2/2  Verify"
  local dmg; dmg="$(find_dmg)"
  [[ -d "$APP_PATH" && -n "$dmg" ]] || fail "Nothing to verify. Run the build step first."

  codesign --verify --deep --strict --verbose=2 "$APP_PATH"
  verify_codesign_matches_profile "$APP_PATH"
  require_entitlements "$APP_PATH" \
    com.apple.security.network.server \
    com.apple.security.application-groups
  spctl -a -vv -t install "$dmg"
  xcrun stapler validate "$dmg"
  xcrun stapler validate "$APP_PATH"
  ok "Developer ID checks passed → ${dmg}"
}

case "${1:-all}" in
  build)  do_build ;;
  verify) do_verify ;;
  all)    do_build; do_verify; echo "🎉  ${APP_NAME} ${BUNDLE_VERSION} is signed, notarized and verified." ;;
  *)      echo "Usage: $0 [build|verify|all]" >&2; exit 1 ;;
esac
