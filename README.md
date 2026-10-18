# PTCGL Tracker

A desktop companion for Pokémon TCG Live. It watches the game window, spots the
match results screen, and saves your battle log to `~/.ptcgl-battle-logs`. It
only reads the screen. It never clicks, types, or reads game memory
([ADR 0001](docs/adr/0001-no-game-automation.md)).

Tauri 2 (Rust) + React, shadcn/ui on Tailwind v4. Battle logs are parsed with
[`@dierkens.dev/ptcgl-battle-log-parser`](https://www.npmjs.com/package/@dierkens.dev/ptcgl-battle-log-parser).
macOS today; Windows is planned. Terms are defined in [GLOSSARY.md](GLOSSARY.md).

### Demo

[![PTCGL Tracker demo](https://github.com/cdierkens/ptcgl-battle-log-tracker/releases/download/v0.1.0/PTCGL.Tracker.Demo.final2.mp4)](https://github.com/cdierkens/ptcgl-battle-log-tracker/releases/tag/v0.1.0)

39 seconds: results screen → Capture Drawer → "Synced to Pokodojo".

## Install (macOS)

Download the latest release from [GitHub Releases](https://github.com/cdierkens/ptcgl-battle-log-tracker/releases):

- **PTCGL Tracker_0.1.0_aarch64.dmg** — disk image, drag to Applications
- **PTCGL Tracker_0.1.0_aarch64.app.zip** — zipped app bundle

On first launch, macOS Gatekeeper will block it (unidentified developer). Right-click the app → **Open**, then click **Open** in the dialog.

This is an unsigned, ad-hoc-signed MVP build. Notarization and auto-updates come in a later release.

## Run from source

## Requirements

- Node 24+ and pnpm 11 (`packageManager` in `package.json`)
- Rust 1.86 (pinned by `rust-toolchain.toml`; rustup installs it)
- macOS with Xcode Command Line Tools

## Run

```bash
pnpm install
pnpm tauri dev                # debug build with the Vite dev server
pnpm tauri dev --release      # what to play on
pnpm tauri build              # a .app bundle in src-tauri/target/release/bundle
```

`pnpm tauri dev` starts Vite (port 5173) and builds the Rust app. Running `cargo`
directly in `src-tauri` needs a built UI in `dist/` first (`pnpm build`), because
`generate_context!` embeds it.

To try log sync without a real site, run `pnpm sync-stub` and paste the config it
prints into Settings → Sync. Only debug builds accept its `http://localhost` URL.
`STUB_STATUS=401 pnpm sync-stub` forces a status (401 shows needs attention).

### Windows support — coming soon

Windows is planned but not yet supported. The non-macOS capture/OCR ports are
stubs, and no Windows binaries are published. Follow-up spec #3 (Windows support)
covers the port; it ships through the release pipeline, not as a cross-compiled
`.exe` from macOS.

If you'd like to help test Windows builds, open an issue and we'll coordinate
once the release spec lands.

## Where things live

- **Saved logs:** `~/.ptcgl-battle-logs` by default. Change it in Settings.
- **App diagnostic log:** `~/.ptcgl-battle-logs/logs/capture.log.<date>`. For
  a trace of what each poll saw (tab seen, or `None` with no frame; active and
  game pid; focus), logged when it changes, run with
  `RUST_LOG=info,ptcgl_battle_log_capture_lib::engine=debug`.
- **Settings:** the OS app-config folder, e.g.
  `~/Library/Application Support/dev.dierkens.ptcgl-tracker/settings.json`.
  Delete it to rerun first-run setup.

See [ADR 0002](docs/adr/0002-log-folder-in-home-settings-in-app-config.md).

## macOS permissions

Screen Recording is requested on first run. In development it's attributed to
the terminal you launched from, so a terminal restart may be needed before the
prompt appears.

## Checks

```bash
pnpm run verify               # typecheck, lint, test, build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs these on macOS, plus a compile-and-test job on Windows.

## Performance notes

**Vision cold start.** The first OCR call in a new binary takes about 35–80 s;
later calls take 0.1–1 s. Every rebuild is a new binary, so right after a build
results detection is blind for about a minute
([#2](https://github.com/cdierkens/ptcgl-battle-log-tracker/issues/2)).

**Debug vs release.** xcap and image build optimised even in dev
(`[profile.dev.package.xcap]`). Without that, xcap's per-pixel copy of the
2560x1496 capture took 145 ms in menus and up to ~1.1 s mid-match, so debug
polls ran about a second apart. With it, capture costs about the same as in
release (~25–80 ms). Measured after the BMP encoder landed (2560x1496 capture):

| | debug | release |
|---|---|---|
| encode a 512x150 region | 2.21 ms | 0.12 ms |
| region OCR | 34.1 ms | 69.5 ms |

Of our own code, only the encoder is meaningfully profile-sensitive, and in
release it costs 0.12 ms, about 2 ms per poll. The OCR is macOS Vision (native) and doesn't speed
up with optimisation. Its per-call cost is large and varies with what's on
screen, so debug and release land within each other's noise.

**Crop size.** Vision's cost isn't proportional to crop size. The 512x150 region
costs ~33 ms, while a 1.5 Mpx band and the 3.8 Mpx whole frame both cost
~250 ms. The curve flattens well before full-frame size, which is why the
safety-net sweep reads a band.

## Bench

```bash
cd src-tauri && cargo run --release --example bench_region
```

Times the detection OCR against a live game window. It needs the game open, and
prints `no window` otherwise.

It reports the region, the three candidate sweep bands, and the whole frame
back-to-back in one run. **Compare within a single run only.** Across runs the
OCR numbers swing by more than 15% with identical code, because Vision's cost
depends on what's on screen. The `encode` lines are deterministic and are the
only figures safe to compare across runs.

## License

[MIT](LICENSE)
