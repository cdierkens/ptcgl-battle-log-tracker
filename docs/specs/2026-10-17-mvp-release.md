# Spec: MVP release — unsigned macOS `.app` via GitHub Releases

Status: Draft
Supersedes: nothing yet (first release spec)
Related: D17 (no binaries until release spec lands), D6 (identifier), D14 (crate name)

## Goal

Ship the first public binary: an unsigned, ad-hoc-signed macOS `.app` (and `.dmg`) available as a GitHub Release download. No auto-updater yet — users download manually. Notarization and the Tauri updater (`latest.json`) are deferred to the full release spec.

## Why MVP first

D17 requires a release spec before any public binary. This spec satisfies that gate minimally so the author (and any early testers) can install the app without waiting for notarization infrastructure. Gatekeeper will flag the app as "unidentified developer"; users right-click → Open.

## Scope (in)

- Tag `v0.1.0` on `main`
- Build macOS release bundle via `pnpm tauri build`
- Upload `.app` (and `.dmg`) as GitHub Release assets
- README download link + Gatekeeper instructions

## Scope (out)

- Apple Developer ID signing / notarization
- Tauri auto-updater (`latest.json`, public key embedded in binary)
- Windows binaries (spec #3)
- `.msi` or Windows installer

## Acceptance

- [ ] `v0.1.0` tag exists on `main`
- [ ] GitHub Release `v0.1.0` has macOS `.app` and `.dmg` assets
- [ ] README links to the release and documents Gatekeeper workaround
- [ ] `pnpm run verify` passes on `main` at the tag
