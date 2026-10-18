# Testing Windows Builds

Since you're on macOS, you have three options:

---

## Option 1: GitHub Actions (Easiest, Free)

The `compile-windows` job in `.github/workflows/verify.yml` already runs on every PR/push to main. It:
- Builds the frontend (`pnpm build`)
- Runs `cargo check --all-targets` + `cargo test` on `windows-latest`

**To get a runnable `.exe`**, add a build step that uploads the artifact:

```yaml
# Add to compile-windows job after "Cargo test":
- name: Build release binary
  run: cargo build --release
  working-directory: src-tauri

- name: Upload Windows binary
  uses: actions/upload-artifact@v4
  with:
    name: ptcgl-tracker-windows
    path: src-tauri/target/release/ptcgl-battle-log-capture.exe
    retention-days: 7
```

Then download the artifact from the Actions run page.

---

## Option 2: Cross-Compile from macOS (Using `cross`)

```bash
# Install cross (uses Docker)
cargo install cross --git https://github.com/cross-rs/cross

# Add Windows target
rustup target add x86_64-pc-windows-gnu

# Build (requires Docker running)
cross build --release --target x86_64-pc-windows-gnu
# Output: target/x86_64-pc-windows-gnu/release/ptcgl-battle-log-capture.exe
```

**Caveats:**
- Requires Docker Desktop
- `windows` crate may need `x86_64-pc-windows-msvc` target instead (MSVC toolchain)
- Tauri bundler won't create `.msi`/`.exe` installer from macOS

---

## Option 3: Build Inside UTM VM (Native Windows Build)

**Prerequisites in the VM:**
```powershell
# 1. Install Rust
winget install Rustlang.Rust.GNU

# 2. Install Node.js (via fnm or direct)
winget install Schniz.fnm
fnm install 24
fnm use 24

# 3. Install pnpm
corepack enable
corepack prepare pnpm@11 --activate

# 4. Install Tauri CLI
cargo install tauri-cli --version "^2"

# 5. WebView2 Runtime (required for Tauri)
winget install Microsoft.EdgeWebView2Runtime
```

**Build:**
```powershell
# Clone or copy the repo to VM
git clone https://github.com/cdierkens/ptcgl-battle-log-tracker.git
cd ptcgl-battle-log-tracker

pnpm install --frozen-lockfile
pnpm tauri build
# Output: src-tauri/target/release/bundle/msi/... or .exe
```

---

## Recommendation

**For quick iteration**: Use **Option 1 (GitHub Actions)** — push a branch, download the `.exe` artifact, copy to UTM VM via shared folder, test.

**For debugging**: Use **Option 3 (UTM VM)** — build natively inside Windows, attach debugger, test overlay windows, OCR, etc.

---

## Quick Test Checklist (when you have a Windows binary)

- [ ] App launches without crashing
- [ ] Settings window opens
- [ ] Can pick log folder (Documents or AppData)
- [ ] Engine starts: "Waiting for the game window"
- [ ] PTCG Live window detected (title match)
- [ ] OCR works: results screen → "Battle Log" / "Export" detected
- [ ] Capture drawer appears (transparent, click-through shield)
- [ ] Clipboard auto-save works
- [ ] Sync: add destination, paste token, save log → delivers
- [ ] No keychain prompts (encrypted file store)

---

## Current Windows Support Status

| Feature | Status |
|---------|--------|
| Compile | ✅ CI passes |
| Unit tests | ✅ CI passes |
| OCR (Windows.Media.Ocr) | ✅ Implemented |
| Screen capture (Graphics Capture API via xcap) | ✅ Should work |
| Overlay windows (Tauri WebView) | ⚠️ Needs testing |
| Secret store (encrypted file) | ✅ Implemented |
| Log folder (AppData/Documents) | ⚠️ Needs verification |
| Bundler (.msi/.exe) | ❌ Only works on Windows |

The `compile-windows` job keeps the code compiling. The missing piece is **running the binary on real Windows** to verify overlays, OCR accuracy, and window detection.
