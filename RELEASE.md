# Release Process

## Prerequisites

1. **Apple Developer ID certificate** exported as `.p12`
2. **App-specific password** for notarization (generated at appleid.apple.com)
3. **GitHub secrets** configured in the `awake` repo:
   - `APPLE_CERTIFICATE_BASE64` — base64-encoded `.p12` certificate
   - `APPLE_CERTIFICATE_PASSWORD` — password for the `.p12` file
   - `APPLE_ID` — Apple ID email
   - `APPLE_TEAM_ID` — Apple Developer Team ID
   - `APPLE_APP_PASSWORD` — app-specific password
   - `HOMEBREW_TAP_TOKEN` — GitHub PAT with push access to `anatomic/homebrew-awake`
4. **Homebrew tap repo** `anatomic/homebrew-awake` exists on GitHub

## Creating a Release

1. Update version in `Cargo.toml`
2. Commit: `git commit -am "Bump version to X.Y.Z"`
3. Tag: `git tag vX.Y.Z`
4. Push: `git push origin main --tags`

The GitHub Actions workflow will automatically:
- Build a universal binary (arm64 + x86_64)
- Create the `.app` bundle
- Sign and notarize with Apple
- Create a GitHub Release with the ZIP artifact
- Update the Homebrew cask in `anatomic/homebrew-awake`

## Local Signing (optional)

```sh
export DEVELOPER_ID_APPLICATION="Developer ID Application: Your Name (TEAM_ID)"
export APPLE_ID="your@email.com"
export APPLE_TEAM_ID="ABCDE12345"
export APPLE_APP_PASSWORD="xxxx-xxxx-xxxx-xxxx"

make sign
```

## Encoding the Certificate

```sh
base64 -i certificate.p12 | pbcopy
```

Paste the result into the `APPLE_CERTIFICATE_BASE64` GitHub secret.
