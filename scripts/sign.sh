#!/usr/bin/env bash
set -euo pipefail

APP_PATH="${1:?Usage: sign.sh <path-to-app>}"

# Required environment variables
: "${DEVELOPER_ID_APPLICATION:?Set DEVELOPER_ID_APPLICATION to your signing identity}"
: "${APPLE_ID:?Set APPLE_ID to your Apple ID email}"
: "${APPLE_TEAM_ID:?Set APPLE_TEAM_ID to your Apple Team ID}"
: "${APPLE_APP_PASSWORD:?Set APPLE_APP_PASSWORD to an app-specific password}"

ENTITLEMENTS="$(dirname "$0")/entitlements.plist"

echo "==> Signing ${APP_PATH}..."
codesign --force --deep --options runtime \
    --entitlements "${ENTITLEMENTS}" \
    --sign "${DEVELOPER_ID_APPLICATION}" \
    "${APP_PATH}"

echo "==> Verifying signature..."
codesign --verify --deep --strict "${APP_PATH}"

echo "==> Creating ZIP for notarization..."
ZIP_PATH="${APP_PATH%.app}.zip"
ditto -c -k --keepParent "${APP_PATH}" "${ZIP_PATH}"

echo "==> Submitting for notarization..."
xcrun notarytool submit "${ZIP_PATH}" \
    --apple-id "${APPLE_ID}" \
    --team-id "${APPLE_TEAM_ID}" \
    --password "${APPLE_APP_PASSWORD}" \
    --wait

echo "==> Stapling notarization ticket..."
xcrun stapler staple "${APP_PATH}"

echo "==> Verifying notarization..."
spctl -a -vv "${APP_PATH}"

echo "==> Re-creating ZIP with stapled ticket..."
rm -f "${ZIP_PATH}"
ditto -c -k --keepParent "${APP_PATH}" "${ZIP_PATH}"

echo "==> Done: ${ZIP_PATH}"
