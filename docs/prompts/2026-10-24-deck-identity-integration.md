# Planning prompt — integrate deck identity into PTCGL Tracker

## Role

You are planning, not implementing. Produce a grilled plan and draft spec for
integrating the extra deck-identity pieces from
`@dierkens.dev/ptcg-decklist-tools` into PTCGL Tracker.

Do not edit application code in this task. The deliverable is a plan that a
later implementation can follow. If a decision is hard to reverse — especially
the envelope shape or where deck metadata is stored — recommend an ADR.

The portable destination payload is the player's confirmed **decklist**, **format**,
and **archetype**, plus an optional **opponent archetype** when it is known.
Both PokeDojo and Training Court should receive that payload. `deckId`,
`listId`, `catalogVersion`, and `catalogContextId` are local metadata or
optional extra fields unless a destination proves it needs them; do not make
opaque ids the only identity.

Opponent archetype is match metadata, not part of the player's deck identity.
The tracker has no archetype-profile engine today; the parser does not detect
archetypes. Treat opponent archetype as optional and never let it block a save.

## Read first

In `ptcgl-battle-log-tracker`:

- `AGENTS.md`
- `GLOSSARY.md`
- `docs/adr/0001-no-game-automation.md`
- `docs/adr/0002-log-folder-in-home-settings-in-app-config.md`
- `docs/adr/0005-uploads-deliver-saved-logs.md`
- `docs/specs/2026-10-12-log-sync.md`
- `docs/specs/2026-10-17-mvp-release.md`
- `src/lib/use-matches.ts`, `src/lib/match-row.ts`, `src/lib/tauri.ts`
- `src-tauri/src/logs.rs`, `sink.rs`, `delivery_queue.rs`, `sync_worker.rs`,
  `commands.rs`, `settings.rs`, `ports.rs`

In `ptcg-decklist-tools`:

- `README.md`
- `docs/specs/2026-10-23-decklist-identity.md`
- `docs/adr/0009-catalog-context-travels-with-deck.md`
- `docs/adr/0010-normalized-deck-owns-projections.md`
- `docs/adr/0011-deck-view-and-structured-evidence.md`
- `docs/adr/0012-catalog-build-runtime-contract.md`

In `pokedojo.pro`:

- `docs/specs/2026-10-12-tracker-log-sync.md` — the receiving side, especially
  Synced log, Game, `savedAt`, and the schema-1 envelope.

In `training-court`:

- `README.md`
- `TRACKER-SYNC-PROPOSAL.md`
- `TRACKER-SYNC-AUDIT.md`
- `TRACKER-SYNC-ISSUE.md`

The Training Court proposal is the current v1 surface: per-device default
format, no decklist sent, and no new tracker release. This task changes that
assumption; treat the proposal as prior art to revise, not a fixed contract.

Use the tracker vocabulary for capture and sync: **Saved log**, **Match**,
**Capture**, **Capture drawer**, **Destination**, **Envelope**, **Delivery**.
Use the decklist-tools vocabulary for identity: **Decklist**, **Deck**,
**Printing**, **Functional card**, **Deck identity**, **List identity**,
**Format window**, **Archetype alias**, **Unresolved**.

Do not call the player's deck identity "opponent archetype." They are separate
fields: the player's archetype describes the confirmed decklist; the opponent's
archetype is match metadata. The parser does not detect either; PokeDojo and
Training Court have their own opponent-archetype profiles or editable fields.

## What exists today

- PTCGL Tracker saves raw battle-log text. A `Match` is a saved log plus a
  parser summary derived in the React webview.
- There is no decklist, deck library, active deck, or player-deck identity in
  the tracker today.
- Sync sends one envelope from Rust:
  `{ schemaVersion: 1, log, playerName?, savedAt, appVersion }`.
- PokeDojo stores an arriving log as a **Synced log**, pending deck assignment.
  It becomes a **Game** only when the player assigns a deck. Its current
  preview carries opponent archetype, not the player's decklist, format, or
  confirmed archetype.
- Training Court inserts a synced log as a normal row in `logs`; the row has
  `archetype`, `opp_archetype`, `format`, and `decklist_id` columns. Its edit-log
  dialog already exposes My deck, Decklist, Opponent's deck, and Format. The
  current tracker proposal sends none of those: format is a per-device default
  and decklist stays empty.
- The battle-log parser has no archetype detection. PokeDojo detects opponent
  archetype from its profiles; Training Court stores `opp_archetype` and lets
  the player edit the opponent's deck. The tracker has neither profile data nor
  an opponent-archetype picker today.
- Decklist parsing, normalisation, identity, format windows, and archetype
  suggestions are TypeScript-only in `@dierkens.dev/ptcg-decklist-tools@0.2.0`.
  The tracker is Tauri 2 + React; the sync worker is Rust.

## The identity pieces to plan around

The package exposes:

- `parseDecklist(text)` → `Result<ParsedDecklist, DecklistError[]>`
- `normalizeDecklist(parsed, { catalog })` → `Result<NormalizedDeck, DecklistError[]>`
- `deckId(deck)` — functional identity, stable across printings
- `listId(deck)` — exact-printing identity
- `canonicalText(deck)` — the `deckId` pre-image
- `canonicalText` also exists for debug/goldens; `deckView(deck)` exposes
  read-only sections, functional/list counts, and name counts
- `countByName(deck)` — the four-copy check, separate from identity
- `parseFormatLabel(label)` / `validateDeck(deck, window)` / `fittingWindows(deck, { era })`
- `suggestArchetype(deck, options)` — advisory; carries structured `evidence`
  and prose `reasons`
- `diffDecklists(a, b, options)`
- `renderDecklist(deck, style, options)`

Important invariants:

- IDs are only comparable within the same `(hashVersion, catalogVersion)`.
- `NormalizedDeck` carries `catalogVersion` and, for newly normalised decks,
  `catalogContextId`. A restored custom deck needs its catalog; missing or
  mismatched context throws `CatalogContextError`.
- `deckId` treats a printing swap as the same deck; `listId` treats it as a
  different list.
- A format window sits beside identity, not inside it. `fittingWindows` returns
  candidates; it never chooses the base for the player.
- Archetype suggestions are never auto-committed. Store the confirmed
  `{ id, label, cards }` separately.
- Unresolved cards still hash under a provisional key; they must remain visible
  as unresolved, never silently invented.

## The central problem

A battle log does **not** contain the player's decklist. The plan must decide
where a decklist comes from, when it is associated with a match, and what part
of it travels to a destination.

Do not assume OCR, game automation, or reading decklists from PTCG Live. ADR
0001 forbids input automation; player-supplied text is the safe source. The
likely sources are a pasted PTCGL export, a pasted Limitless list, or a typed
list. Evaluate them rather than assuming one.

## Recommended starting hypothesis (challenge it)

Use this as a bias, not a settled answer:

- A player-supplied deck library, with one active deck and a per-match override.
- Compute identity in the webview with `@dierkens.dev/ptcg-decklist-tools`; persist
  serializable identity for the Rust delivery worker. Do not port the parser to
  Rust.
- Store the deck library and match association in app-data SQLite, not beside
  the saved logs. Keep the saved log as the source of truth.
- Build the local deck library, active deck, match association, and match-detail
  display first. The destination payload is the second slice, but design it now
  so the local model stores everything it will need.
- The destination payload is the confirmed decklist, format, and archetype,
  with an optional opponent archetype when the tracker can obtain one.
  A likely portable decklist is `renderDecklist(deck, "ptcgl")`; evaluate
  canonical text or normalized JSON only if a destination needs it.
- Do not build a new archetype-profile engine in the tracker just to fill
  opponent archetype. Start with null/unknown, or a player-selected value if
  the UI needs one; let the destination detect or edit it.
- Treat `deckId`/`listId` and the catalog version as local metadata, or as
  optional extra fields. They are useful for grouping and debugging, but the
  destinations should not have to interpret opaque ids to use the deck.
- Both PokeDojo and Training Court need receiving-side changes for this payload.
  Put the extension behind a destination capability or feature flag; do not
  silently break older destinations.
- Store both `deckId` and `listId`. Group by `deckId`; use `listId` to explain
  printing changes. Never compare ids across catalog versions.

## Decisions the plan must make

### 1. Deck source and deck library

- Where does the player add a decklist? A dedicated deck library in the main
  window, the capture drawer, Settings, or a paste flow?
- Is there one **active deck**, a deck per session, or a deck assigned after
  each match?
- What happens when the player changes decks mid-session?
- How are duplicate decklists handled? Use `deckId` to group the same functional
  deck and `listId` to distinguish printings.
- What is shown for unresolved cards, missing refs, unknown sets, or a catalog
  that predates the list?
- Is the decklist stored as raw text, normalized JSON, both, or a rendered
  canonical form?

### 2. Match association

- Is a saved log stamped with the active deck at save time, or assigned later?
- How do Undo, Delete, Trash, and re-save interact with the association?
- What does the match row show when there is no deck? What does the detail show?
- Can an older match be assigned a deck after the fact? If so, does that update
  only local metadata or also a destination?
- If the player edits the deck later, does an already-saved match keep the old
  decklist/format/archetype snapshot, or follow the current deck? Prefer a
  snapshot at save or association time, like PokeDojo's version-at-`savedAt`.
- How does the association survive moving the log folder or restarting the app?

### 3. Where identity is computed and stored

- The identity library is TypeScript. The sync worker is Rust. Choose the seam:
  compute in the webview and persist serializable identity for Rust; read a
  sidecar from Rust; or move envelope construction to the webview.
- Do not port the decklist parser to Rust as a first move. Explain why if you
  recommend otherwise.
- Where does the deck library live: app-data SQLite (the delivery queue already
  uses SQLite), app-config JSON, or a sidecar beside each saved log? Respect ADR
  0002: logs are player data in the home folder; settings are app state.
- The deck library must store the decklist source and normalized form. The
  per-match record should store the portable payload snapshot (decklist,
  format, archetype) or a versioned reference that survives later deck edits.
  Decide which, and store `deckId`, `listId`, `hashVersion`, `catalogVersion`,
  and `catalogContextId` beside it.
- How are existing saved logs with no identity represented and migrated? No
  backfill can invent a decklist.

### 4. Portable destination payload

The payload is explicitly:

- **Decklist** — the player's confirmed list. Choose the portable shape: raw
  pasted text, `renderDecklist(deck, "ptcgl")`, canonical text, normalized
  JSON, or both text and metadata. State which one is authoritative.
- **Format** — the player-confirmed format window or label. Map the package's
  `SET-SET` window to Training Court's `format` values and PokeDojo's format
  storage. Do not infer the base from the deck; `fittingWindows` returns
  candidates, and the player chooses.
- **Archetype** — the player's confirmed deck archetype, not the parser's
  opponent archetype. Decide whether to send the alias id, label, icon cards,
  or all three, and how each destination maps that field.
- **Opponent archetype (optional)** — match metadata, not deck identity. Decide
  whether the tracker can supply it at all: leave it null, let the player pick
  it, or forward a value detected elsewhere. Do not infer it from the player's
  decklist, and do not add an archetype-profile engine in this slice. Map it to
  PokeDojo's opponent-archetype preview and Training Court's `opp_archetype`.

Then decide:

- Are `deckId`, `listId`, `hashVersion`, `catalogVersion`, and
  `catalogContextId` sent too, or kept local? If sent, they are metadata for
  dedupe/debugging, never the only way to use the deck.
- Is the payload an additive optional field on schema 1 or a schema-2 bump?
  PokeDojo ignores excess properties; Training Court and other destinations may
  be stricter. Decide whether this needs destination capability negotiation, a
  config version bump, or a feature flag.
- Does the log remain the idempotency key? What happens if the same log is
  re-sent with different deck metadata?
- What are the receiving-side changes? PokeDojo needs a player-decklist/format/
  archetype path beside Synced log and Game, and should accept a null/absent
  opponent archetype. Training Court's `logs` row needs a real `format`, an
  archetype value, an optional `opp_archetype`, and a `decklist_id` or a created
  decklist; its current v1 proposal assumes device-default format and no
  decklist.
- What is the null/unknown contract for each field? In particular, an absent
  opponent archetype must not become a guessed value, and an unresolved card in
  the decklist must remain unresolved.
- Should older destinations receive no new fields? Should the tracker hide the
  deck section for destinations that do not advertise support?
- What is explicitly out of scope for the tracker, and what must be a separate
  PokeDojo/Training Court change?

### 5. UI and player flow

- Sketch the smallest flow that lets a player add a deck, mark it active, save
  a match, and see the deck on the match. Do not block saving or the Continue
  shield on deck selection.
- Where do format-window candidates, the decklist preview, the player's
  archetype suggestions, and the optional opponent archetype appear? The player
  confirms each; nothing is auto-committed.
- How are `deckId` and `listId` differences explained when a player swaps
  printings?
- How does the UI behave when the catalog updates and an old deck no longer has
  a matching context? `CatalogContextError` should become a clear player action,
  not a crash.
- What is shown in the capture drawer versus the main window versus match detail?

### 6. Compatibility and migration

- Envelope schema 1 is approved. Decide whether to keep it additive or version
  it, and document the compatibility promise.
- Existing saved logs have no deck identity. Decide how they appear and whether
  the player can assign a deck later.
- Existing destinations must keep working. No destination should receive a body
  it cannot parse, and a delivery failure must never lose a saved log (ADR 0005).
- Training Court's current proposal has per-device default format and an empty
  decklist. This direction supersedes that assumption; update the proposal,
  audit, and maintainer conversation, and do not claim the existing v1 endpoint
  accepts a decklist unchanged.
- `deckId`/`listId` values are only comparable within a
  `(hashVersion, catalogVersion)` pair. How does the tracker detect and handle a
  catalog update without silently merging or splitting decks?

### 7. Testing and rollout

- Unit tests for the deck library, storage, identity serialization, match
  association, and envelope serialization.
- Golden tests for fixtures with split printings, localized logs, ref-less
  lists, unknown cards, and rotation windows.
- Rust tests for the envelope's optional fields and queue/migration behavior.
- Manual test against a real PTCGL export and dev PokeDojo and Training Court
  destinations.
- Contract tests for the `decklist`/`format`/`archetype`/`opponentArchetype`
  field mapping to both destinations, including absent/unknown values and older
  destinations that ignore the fields.
- Rollout: feature flag, local-only first, sync-extension second, or both at
  once? Recommend a sequence that keeps the first PR reviewable and does not
  claim a destination accepts the payload before its side ships.

## Constraints

- ADR 0001: the app observes the game and clipboard; it never clicks, types, or
  automates PTCGL.
- ADR 0002: saved logs live in the player's chosen log folder; settings live in
  OS app config. Do not move logs into app data.
- ADR 0005: save first, deliver separately. Deck identity must never gate or
  delay saving.
- The raw log remains the source of truth. Identity and opponent archetype are
  derived metadata. A missing opponent archetype is a valid state.
- `@dierkens.dev/ptcg-decklist-tools` is the identity source; do not fork or
  reimplement it. Pin an exact version like the tracker's other dependencies.
  The package is ESM, zero-runtime-dependency, Node >=22.12, and ships its
  catalog on the `./data` subpath; do not fetch catalog data at runtime. If a
  package gap appears, propose an upstream change.
- Treat a decklist as player data. Do not send it to a destination without an
  explicit player choice and visible confirmation. The targeted payload is
  decklist + format + archetype for both PokeDojo and Training Court; an
  identity-only payload is not the default assumption.
- Zero automation of the game. Clipboard reading is opt-in and already exists;
  decklist input is player-supplied text.
- Keep shadcn standard theme tokens (ADR 0003) and the existing capture drawer
  behavior. Do not turn the Continue shield into a deck picker.
- The tracker's `pnpm run verify` and Rust `cargo fmt --check`, `cargo clippy
  --all-targets -- -D warnings`, and `cargo test` must remain the gates.
- Each implementation slice runs the `code-review` skill against the base branch
  before opening or merging its PR, per `AGENTS.md`.

## Process

If the `grilling` or `to-spec` skill is available, use it. Otherwise follow the
frontier-round format in `.agents/skills/grilling/SKILL.md`.

1. Read the docs above and map the current flow from `submit_battle_log` to
   `SyncingLogSink`, the delivery queue, and `sync_worker`.
2. Identify the facts the code already answers; do not ask the user for those.
3. Work the decision tree in rounds. Ask only the frontier questions whose
   prerequisites are settled. Give each question a recommended answer. Wait for
   answers before the next round.
4. Challenge assumptions against `GLOSSARY.md` and the decklist-tools glossary.
   In particular, do not blur Decklist, Deck, Printing, Deck identity, List
   identity, Format window, and Archetype alias.
5. After the decisions are shared, produce the plan.

## Deliverable

Create a draft spec and plan at
`docs/specs/<date>-deck-identity-integration.md` (or a better path if the repo
has a convention). Include:

- A one-paragraph recommendation and explicit scope boundaries.
- A decision table with options, recommendation, confidence, and reversibility.
- The deck-library and per-match data model, including migration for logs with
  no identity.
- The chosen TypeScript/Rust seam and why.
- The proposed envelope delta and a field-mapping table:
  tracker field → PokeDojo field → Training Court field for decklist, format,
  player archetype, and opponent archetype, including nulls, unresolved cards,
  and catalog mismatch.
- The receiving-side follow-up for both PokeDojo and Training Court, with
  compatibility behavior for older destinations.
- The player flow and UI states, including no-deck, unresolved, catalog-update,
  and context-mismatch states.
- The test plan and manual verification steps.
- Risks, non-goals, and open questions.
- A staged issue breakdown, each slice independently reviewable and shippable.
  Suggested slices: deck library + local identity; active deck + match
  association; match-detail UI; portable payload + capability flag; PokeDojo
  receiving follow-up; Training Court receiving follow-up.

Do not start implementation until the plan is approved.
