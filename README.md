# amux

amux is a native macOS workspace for managing multiple coding agents and worktrees in one place.

## Development

Build locally:

```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build
```

Run UI tests:

```bash
./run_ui_tests.sh
```

Build a release zip for the current machine architecture:

```bash
./scripts/package_release.sh
```

The packaged artifact is written to `dist/`.

## GitHub Releases

This repository includes a GitHub Actions workflow at `.github/workflows/release.yml`.

- Pushing a tag like `v2.0.0` triggers a release build.
- The workflow builds both `arm64` and `x86_64` macOS artifacts.
- It publishes `amux-macos-arm64.zip` and `amux-macos-x86_64.zip` to the GitHub Release.

### Optional notarization

If the following repository secrets are configured, the workflow signs, notarizes, staples, and then uploads notarized artifacts:

- `APPLE_CERTIFICATE_P12`
  Base64-encoded Developer ID Application certificate (`.p12`)
- `APPLE_CERTIFICATE_PASSWORD`
  Password for the `.p12`
- `APPLE_DEVELOPER_IDENTITY`
  Full codesigning identity name, for example `Developer ID Application: Your Name (TEAMID)`
- `APPLE_ID`
  Apple ID used for notarization
- `APPLE_APP_SPECIFIC_PASSWORD`
  App-specific password for that Apple ID
- `APPLE_TEAM_ID`
  Apple Developer Team ID

Without those secrets, the workflow still builds and uploads unsigned release zips.

## Release process

1. Update the version in `project.yml`.
2. Commit and push to the default branch.
3. Create and push a tag, for example `git tag v2.0.0 && git push origin v2.0.0`.
4. Wait for the `Release` workflow to finish.
5. Verify the GitHub Release assets and release notes.
