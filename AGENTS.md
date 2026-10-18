# ptcgl-battle-log-tracker

Desktop companion ("PTCGL Tracker") that watches Pokémon TCG Live, spots the
match results screen, and saves the battle log. Tauri 2 (Rust) + React. Public,
MIT. Sibling of `@dierkens.dev/ptcgl-battle-log-parser`.

## Status: standalone

Extracted from `pokedojo.pro/apps/battle-log-capture` per
`docs/specs/2026-10-11-capture-app-extraction.md`. Builds on its own; macOS
only for now (Windows compiles in CI with stubbed capture/OCR).

## Commands

- `pnpm install`, `pnpm tauri dev`
- `pnpm run verify`: typecheck, lint, test, build. Run it before committing.
- `pnpm build:windows`: cross-compile the Windows `.exe` on macOS via cargo-xwin (setup in README).
- In `src-tauri/`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. Cargo needs `dist/` from `pnpm build`.
- Dependencies are pinned exactly. pnpm 11 rejects releases younger than a day.
- Run the `code-review` skill against the base branch after each layer (each issue/PR in a stack), and resolve its findings before opening or merging that PR.

## Conventions

- Plain TypeScript + React. **No Effect**, no `@pokedojo/*`, no PokeDojo rules.
- Parsing: npm `@dierkens.dev/ptcgl-battle-log-parser` (`Result` union, no throw).
- UI: shadcn/ui on Tailwind v4. Use shadcn's standard tokens only (ADR 0003), and leave `components/ui/*` as shadcn generates it.
- Bundle identifier `dev.dierkens.ptcgl-tracker`. It is permanent once binaries ship.
- Conventional commits.

## Docs

- `GLOSSARY.md`: domain vocabulary. Use its terms; avoid the listed synonyms.
- `docs/specs/`: specs. Draft until grilled, then approved.
- `docs/adr/`: decisions. ADR 0001 (no game automation) binds every feature.

## Agent skills

Committed under `.agents/skills/`, sourced via `skills-lock.json`. Update
them deliberately with `npx skills update`, and review the diff.
