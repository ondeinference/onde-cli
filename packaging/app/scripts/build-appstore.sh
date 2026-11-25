#!/usr/bin/env bash
# Mac App Store pipeline for Onde Desktop.
#   build   frontend, then each architecture with tauri build --no-bundle, lipo,
#           tauri bundle --no-sign, and codesign (universal, App Store config,
#           YYYYMMDD.HHMM build number). The script runs lipo and codesign itself
#           because tauri-cli cannot do either for this app from a non-tty shell.
#   sign    productbuild, signed with the 3rd Party Mac Developer Installer cert
#   upload  altool upload to App Store Connect
#
# Usage: scripts/build-appstore.sh [build|sign|upload|all]
#
# Environment (all optional unless noted):
#   ASC_API_KEY, ASC_ISSUER_ID   required for upload; the .p8 is read from
#                                API_PRIVATE_KEYS_DIR or ~/.appstoreconnect/private_keys
#   MAS_PROVISIONING_PROFILE_PATH  default: the private apple-certificates checkout
#   SIGNING_IDENTITY             installer identity; default: looked up in the keychain
#   MAS_SIGNING_IDENTITY         app identity; default: signingIdentity in the App Store config
#   DEVELOPER_DIR                a release Xcode; default: picked automatically
#   BUNDLE_VERSION               default: now, UTC
set -euo pipefail
# shellcheck source=release-lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/release-lib.sh"

CONFIG="${TAURI_DIR}/tauri.prod-macos-appstore.conf.json"
MAS_PROVISIONING_PROFILE_PATH="${MAS_PROVISIONING_PROFILE_PATH:-${HOME}/Repositories/apple-certificates/onde-desktop/OndeDesktop_MacAppStore.provisionprofile}"
TARGET_DIR="${CARGO_TARGET_DIR:-${TAURI_DIR}/target}"
[[ "$TARGET_DIR" = /* ]] || TARGET_DIR="${APP_DIR}/${TARGET_DIR}"
APP_PATH="${TARGET_DIR}/universal-apple-darwin/release/bundle/macos/${APP_NAME}.app"
PKG_PATH="${APP_DIR}/${APP_NAME}.pkg"

do_build() {
  step "1/3  Build"
  require_release_xcode
  load_dotenv
  require_profile "$MAS_PROVISIONING_PROFILE_PATH" "Mac App Store"

  BUNDLE_VERSION="${BUNDLE_VERSION:-$(bundle_version)}"
  export_build_number "$BUNDLE_VERSION"
  echo "🏷   bundleVersion → ${BUNDLE_VERSION}"
  echo "📜  profile       → ${MAS_PROVISIONING_PROFILE_PATH}"

  build_frontend
  build_arch_binary aarch64-apple-darwin --config "$CONFIG"
  build_arch_binary x86_64-apple-darwin  --config "$CONFIG"
  METALLIB="$(mlx_metallib)"
  echo "🧩  mlx.metallib  → ${METALLIB}"
  lipo_universal "$TARGET_DIR"

  # --config values merge in order; the file is never modified. --no-sign
  # because codesign_app signs the bundle below.
  step "Bundle"
  rm -rf "$APP_PATH"
  (cd "$APP_DIR" && pnpm tauri bundle \
    --bundles app \
    --target universal-apple-darwin \
    --no-sign \
    --config "$CONFIG" \
    --config "$(config_json "$BUNDLE_VERSION" "$MAS_PROVISIONING_PROFILE_PATH" "$METALLIB")")

  [[ -d "$APP_PATH" ]] || fail "Bundling succeeded but the .app is not at: ${APP_PATH}"
  require_metallib "$APP_PATH"
  codesign_app "$APP_PATH" \
    "${MAS_SIGNING_IDENTITY:-$(config_signing_identity "$CONFIG")}" \
    "${TAURI_DIR}/Entitlements-appstore.plist"
  verify_codesign_matches_profile "$APP_PATH"
  require_entitlements "$APP_PATH" \
    com.apple.security.app-sandbox \
    com.apple.security.network.client \
    com.apple.security.network.server \
    com.apple.security.application-groups
  ok "Build complete → ${APP_PATH}"
}

resolve_installer_identity() {
  [[ -z "${SIGNING_IDENTITY:-}" ]] || return 0
  local label="3rd Party Mac Developer Installer"
  SIGNING_IDENTITY="$(security find-identity -v -p basic 2>/dev/null \
    | awk -F'"' -v want="$label" '$0 ~ want {print $2; exit}')"
  [[ -n "$SIGNING_IDENTITY" ]] \
    || fail "No usable '${label}' identity in the keychain.\n       Import it, or set SIGNING_IDENTITY."
}

do_sign() {
  step "2/3  Sign"
  [[ -d "$APP_PATH" ]] || fail ".app not found: ${APP_PATH}\n       Run the build step first."
  resolve_installer_identity
  echo "🔏  identity → ${SIGNING_IDENTITY}"
  rm -f "$PKG_PATH"
  xcrun productbuild --sign "$SIGNING_IDENTITY" --component "$APP_PATH" /Applications "$PKG_PATH"
  [[ -f "$PKG_PATH" ]] || fail "productbuild succeeded but there is no .pkg at: ${PKG_PATH}"
  ok "Signed → ${PKG_PATH}"
}

do_upload() {
  step "3/3  Upload"
  [[ -f "$PKG_PATH" ]] || fail ".pkg not found: ${PKG_PATH}\n       Run the sign step first."
  require_asc_credentials
  echo "📤  file   → ${PKG_PATH}"
  echo "🔑  apiKey → ${ASC_API_KEY}"
  # altool finds AuthKey_<id>.p8 through API_PRIVATE_KEYS_DIR.
  API_PRIVATE_KEYS_DIR="$(dirname "$ASC_KEY_PATH")" xcrun altool --upload-app --type macos \
    --file "$PKG_PATH" --apiKey "$ASC_API_KEY" --apiIssuer "$ASC_ISSUER_ID"
  ok "Uploaded to App Store Connect (note the Delivery UUID above)"
}

case "${1:-all}" in
  build)  do_build ;;
  sign)   do_sign ;;
  upload) do_upload ;;
  all)    do_build; do_sign; do_upload; echo "🎉  ${APP_NAME} ${BUNDLE_VERSION} built, signed and uploaded." ;;
  *)      echo "Usage: $0 [build|sign|upload|all]" >&2; exit 1 ;;
esac
