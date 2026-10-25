# Spec: Deck identity in PTCGL Tracker

Status: **Draft** (grilled 2026-10-24; implementation not started)
Repo: `ptcgl-battle-log-tracker`
Depends on: `@dierkens.dev/ptcg-decklist-tools@0.2.0`
Related: [log sync](2026-10-12-log-sync.md), [capture app extraction](2026-10-11-capture-app-extraction.md), [MVP release](2026-10-17-mvp-release.md)
Receiving sides: `pokedojo.pro/docs/specs/2026-10-12-tracker-log-sync.md`, `training-court/TRACKER-SYNC-PROPOSAL.md`
Vocabulary: [GLOSSARY.md](../../GLOSSARY.md), plus the decklist-tools glossary

## Goal

Let the player keep decklists in the tracker, mark one active, and attach a
snapshot of **decklist**, **format**, **player archetype**, and optional
**opponent archetype** to a saved log. Send that portable payload to both
PokeDojo and Training Court when the destination supports it, without changing
the raw log or blocking a save.

The local flow is useful on its own. Sync is a second, explicit step.

## Non-goals

- Reading decklists from PTCG Live with OCR, keyboard input, or any other
  automation (ADR 0001).
- Inferring the player's decklist from a battle log.
- Authoritative legality or a new archetype-profile engine.
- Auto-committing a format or archetype without the player's confirmation.
- Replacing the raw battle log as the source of truth.
- A new destination config format or a required PokeDojo/Training Court release
  in the same slice as the tracker change.
- Bulk backfill of existing logs or remote edit/delete.
- Opponent-archetype detection in the tracker. The parser does not detect
  archetypes, and the tracker ships no profiles.

## Decisions

| # | Decision | Why |
|---|---|---|
| D1 | Decklists are player-supplied. A deck library accepts a pasted PTCGL export, a Limitless paste, or a typed list. No OCR or game automation. | The battle log does not contain the list; automation is forbidden and fragile. |
| D2 | One active deck, with a per-match override. A saved log gets an immutable snapshot at save or association time. | Back-to-back play uses one deck; later edits must not rewrite old matches. |
| D3 | Store deck library and match snapshots in app-data SQLite. Keep the saved log in the player's log folder untouched. | Deck metadata is app state; logs are player data (ADR 0002). SQLite already backs the delivery queue. |
| D4 | Compute identity in the React webview with `@dierkens.dev/ptcg-decklist-tools`; persist a serializable snapshot for the Rust delivery worker. Do not port the parser to Rust. | The identity package is TypeScript-only. Rust only needs the snapshot for the envelope. |
| D5 | The portable payload is `decklist`, `format`, `archetype`, and optional `opponentArchetype`. Both PokeDojo and Training Court are targets. | These are the fields the destinations can use; opaque IDs are not the only identity. |
| D6 | `decklist` is `renderDecklist(deck, "ptcgl")` text. Store the original source text and the rendered text. | PTCGL text is pasteable and usable by both destinations; the original preserves the player's input. |
| D7 | `archetype` is the player-confirmed alias, stored as `{ id, label, cards }`, or `null` when not confirmed. | Suggestions are advisory; the stable id and label both matter. |
| D8 | `opponentArchetype` is optional match metadata. It is `null` when unknown. No profile engine is added to the tracker. | The parser and tracker have no opponent archetype detection; PokeDojo/Training Court can detect or edit it later. |
| D9 | `format` is the player-confirmed label. `fittingWindows` supplies candidates; the tracker never infers the base. Non-window labels are passed through only after confirmation. | Matches the package contract and Training Court's stricter `format` field. |
| D10 | Add an optional `deck` object to envelope schema 1; do not bump the schema version. Gate sending with a per-destination `sendDeckIdentity` setting, default off. | Schema 1 is approved and PokeDojo ignores excess properties. The toggle protects stricter destinations. |
| D11 | The raw log stays the idempotency key. Identity changes after a successful delivery do not rewrite remote history. A manual sync resends the current snapshot and is subject to the destination's duplicate policy. | No remote-edit contract exists. |
| D12 | PokeDojo and Training Court receiving changes are separate slices. Neither destination auto-creates or auto-assigns a deck without an agreed contract. | The tracker cannot assume a receiving-side behavior that does not exist. |

## Data model

All deck data lives in `<app data>/decks.sqlite`. Schema version starts at 1.

```sql
-- A stable local deck. The package deckId is not the primary key.
CREATE TABLE decks (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  current_version_id TEXT,            -- null until the first version is written
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  archived_at INTEGER
);

-- Immutable identity snapshots. Editing a deck adds a version.
CREATE TABLE deck_versions (
  id TEXT PRIMARY KEY,
  deck_id TEXT NOT NULL REFERENCES decks(id),
  source TEXT NOT NULL,              -- 'ptcgl' | 'limitless' | 'typed'
  raw_text TEXT NOT NULL,            -- exactly what the player pasted
  decklist_text TEXT NOT NULL,       -- renderDecklist(deck, "ptcgl")
  format_label TEXT,                 -- confirmed label, e.g. 'TEF-30C'
  archetype_json TEXT,               -- JSON { id, label, cards } or null
  deck_id_hash TEXT,                 -- package deckId value
  list_id_hash TEXT,                 -- package listId value
  hash_version INTEGER,
  catalog_version TEXT,
  catalog_context_id TEXT,
  created_at INTEGER NOT NULL
);

-- One snapshot per saved log. Later deck edits do not change this row.
CREATE TABLE match_decks (
  log_file_name TEXT PRIMARY KEY,
  deck_id TEXT NOT NULL REFERENCES decks(id),
  deck_version_id TEXT NOT NULL REFERENCES deck_versions(id),
  decklist_text TEXT NOT NULL,
  format_label TEXT,
  archetype_id TEXT,
  archetype_label TEXT,
  archetype_cards TEXT,
  opponent_archetype_label TEXT,
  opponent_archetype_cards TEXT,
  deck_id_hash TEXT,
  list_id_hash TEXT,
  hash_version INTEGER,
  catalog_version TEXT,
  catalog_context_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Active deck and other small app-state values.
CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  value TEXT
);
-- key = 'active_deck_id'
```

Notes:

- `decks.current_version_id` points at the latest version; version rows are
  immutable. It is null only between creating the deck row and writing its first
  version, inside one transaction.
- A deck is **archived**, not hard-deleted, because `match_decks` snapshots
  reference its versions. Archived decks disappear from the picker; their old
  matches keep working.
- `match_decks` stores the full portable snapshot, not just a reference. A deck
  can be edited or deleted without changing an old match.
- `log_file_name` is the key because capture names are unique
  (`ptcgl-<nanos>.txt`) and survive moving the log folder. If a future import
  path can reuse names, add a content hash to the key.
- Existing logs have no `match_decks` row. They show **Deck not set** and can be
  assigned later. No backfill can invent a decklist.
- Undo and Trash delete the `match_decks` row for the log. A re-save creates a
  new row.
- If the deck database cannot be opened, saving a log still succeeds. Match
  identity is optional metadata; a failure is logged and the UI shows the deck
  library as unavailable.

## TypeScript/Rust seam

### TypeScript

The webview owns parsing and identity:

```ts
parseDecklist(text)
normalizeDecklist(parsed, { catalog })
renderDecklist(deck, "ptcgl")
deckId(deck) / listId(deck)
deckView(deck)
fittingWindows(deck, { era })
suggestArchetype(deck)
```

The deck editor confirms:

- a name;
- the parsed/pasted list;
- a format candidate or an explicitly confirmed non-window label;
- a suggested archetype or `null`;
- an optional opponent archetype label, when a later slice adds a picker.

It then sends a `DeckVersionInput` to Rust. The input is JSON-serializable and
contains the fields in `deck_versions`; it does not contain the full catalog or
the normalized object graph.

Identity errors are values, not throws. `CatalogContextError` and unresolved
cards become UI states. A candidate format that the package cannot resolve is
shown as an opaque confirmed label, not silently accepted as a window.

### Rust

Add `deck_store` in `src-tauri`:

- open/migrate `<app data>/decks.sqlite`;
- `list_decks`, `get_deck`, `save_deck_version`, `archive_deck`;
- `get_active_deck`, `set_active_deck`;
- `assign_match_deck`, `clear_match_deck`, `get_match_deck`;
- expose `DeckLibrary` through Tauri state and commands.

The delivery worker reads `match_decks` by log file name when building the
envelope. If the read fails, it sends the base envelope and logs the error; a
deck-database failure never fails a delivery or a save.

No decklist parsing, identity hashing, or archetype matching is added to Rust.

## Portable envelope delta

Schema 1 stays `schemaVersion: 1`. `deck` is optional.

```json
{
  "schemaVersion": 1,
  "log": "<battle log text>",
  "playerName": "<optional>",
  "savedAt": "<RFC 3339>",
  "appVersion": "<semver>",
  "deck": {
    "decklist": "Pokémon: ...\n\nTrainer: ...\n\nEnergy: ...\n\nTotal Cards: 60\n",
    "format": "TEF-30C",
    "archetype": {
      "id": "limitless:284/3",
      "label": "Dragapult Dusknoir",
      "cards": ["dragapult", "dusknoir"]
    },
    "opponentArchetype": null,
    "identity": {
      "deckId": "sha256:…",
      "listId": "sha256:…",
      "hashVersion": 1,
      "catalogVersion": "2026-10-23",
      "catalogContextId": "catalog:…"
    }
  }
}
```

Contract:

- `deck` is omitted entirely when the match has no deck snapshot.
- Inside `deck`, `decklist` and `format` are required. A deck without a
  confirmed format stays local and is not sent as a `deck` object until its
  format is confirmed.
- `archetype` and `opponentArchetype` are nullable.
- `identity` is optional metadata. Destinations must not require it.
- `opponentArchetype` may carry `{ label, cards }` later; v1 sends `null` when
  no label is known.
- The log remains the idempotency key. Re-sending the same log with a different
  `deck` does not create a second remote record.
- A destination that does not accept `deck` must not receive it. The setting is
  per destination.

## Destination field mapping

| Tracker | PokeDojo | Training Court |
|---|---|---|
| `deck.decklist` | New optional hint on Synced log; Game decklist when assigned | `logs.decklist_id` or a decklist created/attached by the ingest route |
| `deck.format` | Synced log/Game format | `logs.format` |
| `deck.archetype.label` / `cards` | Player deck archetype | `logs.archetype` |
| `deck.opponentArchetype.label` | `preview.opponentArchetypeDetected` when accepted | `logs.opp_archetype` |
| `deck.identity` | Optional metadata; do not require | Optional metadata; do not require |

### PokeDojo

Current receiving side stores a Synced log pending deck assignment. Its preview
already carries an opponent archetype. The new fields should be a **hint**:
show the decklist/format/archetype, but keep the player's deck assignment as the
authority. Auto-creating or auto-assigning a Deck is a separate decision and is
not assumed here.

### Training Court

Training Court's `logs` row already has `archetype`, `opp_archetype`, `format`,
and `decklist_id`; its edit-log dialog already exposes My deck, Decklist,
Opponent's deck, and Format. Its current tracker proposal assumes device-default
format and an empty decklist; that assumption is superseded.

Its ingest route must decide whether to:

- create a decklist from `deck.decklist`, then set `decklist_id`;
- attach an existing decklist after matching; or
- store the text as a hint and leave `decklist_id` null.

That is a Training Court change, not a tracker change. The tracker sends text
and never claims a remote `decklist_id`.

## UI

### Deck library

Add a **Decks** tab to the main window. The capture drawer does not need the
full library.

- Empty state: "Add a deck" with a paste box for PTCGL, Limitless, or typed
  lists.
- Add/edit screen: paste, parse, preview with `renderDecklist`, section counts
  from `deckView`, unresolved-card warnings, format candidates, and archetype
  suggestions with their structured evidence. The player confirms each; nothing
  is auto-committed.
- Deck row: name, format, confirmed archetype, active badge.
- Active deck: one per app, switched from the library or a compact drawer
  control.
- Editing a deck creates a new immutable version. Old matches keep their
  snapshots.

### Capture drawer

- Show the active deck name and format as a small control.
- Allow **No deck**.
- Do not block saving, clipboard auto-save, or the Continue shield on deck
  selection.
- If no deck is active, save the log with no `deck` object.

### Match list and detail

- Match row: deck name or **Deck not set**; keep the existing result/opponent
  line.
- Match detail: deck name, format, player archetype, opponent archetype, and a
  **Change deck** action.
- Changing a deck snapshots the current version; it does not mutate the old
  snapshot silently.
- Show unresolved cards and catalog-context warnings with a link to edit the
  deck.
- Existing logs show **Deck not set** and can be assigned later.

### Destinations

- Add a per-destination **Send deck identity** switch in Destinations settings.
  Default off.
- Explain that the decklist is player data and is sent only when the switch is
  on.
- Enable it for PokeDojo and Training Court only after their receiving side
  accepts the fields.
- If a destination rejects the `deck` object, fail the delivery with a readable
  error; the saved log remains local (ADR 0005).

## Compatibility and migration

- Envelope schema 1 stays the version; `deck` is additive and optional.
- PokeDojo's decoder ignores excess properties. Training Court's future endpoint
  must accept `deck`; until then the tracker switch stays off.
- No destination config change is required for v1. A later capability field can
  replace the local switch if more destinations adopt the payload.
- Existing logs have no identity; no backfill.
- Existing databases: create the new SQLite file on first use. A missing or
  corrupt deck database does not affect log capture or sync.
- Package catalog updates can change `deckId`/`listId`. Store
  `(hashVersion, catalogVersion, catalogContextId)` beside every snapshot. A
  deck whose context no longer matches shows a **Re-import decklist** action;
  do not compare old and new ids as if they were the same catalog.

## Testing

### TypeScript

- Deck paste/parse/normalize/render for PTCGL, Limitless, typed, ref-less, and
  localized lists.
- Split printings: `deckId` stable, `listId` different.
- Unresolved cards remain unresolved and are shown in the editor.
- Format candidates: player confirmation; a non-window label is passed through
  only after confirmation.
- Archetype suggestions remain advisory; confirmed alias is stored.
- Deck edit creates a new version; an old match keeps its snapshot.
- Active deck selection and **No deck**.

### Rust

- `decks.sqlite` schema/migration and CRUD.
- `match_decks` assignment, clear, undo, and trash behavior.
- Envelope with no deck, with deck, with null archetype, with null opponent
  archetype, and with identity metadata.
- Deck-database failure falls back to the base envelope.
- Existing queue behavior is unchanged.

### Contract

- Envelope fixture for PokeDojo schema 1 with and without `deck`.
- Field-mapping fixtures for PokeDojo and Training Court.
- A destination that ignores unknown fields keeps working with the switch off.
- A destination that rejects the payload fails only that delivery.

### Manual

- Add a deck from a real PTCGL export.
- Select it active, save a real battle log, confirm the match shows the snapshot.
- Edit the deck; confirm the old match does not change.
- Assign an older match; confirm the new snapshot.
- Undo and Trash a log; confirm the association is removed.
- Sync to a dev PokeDojo and a dev Training Court with the switch on and off.
- Restart the app and confirm the library, active deck, and snapshots survive.

## Rollout slices

### Slice 1 — Deck library and local identity

- Add `@dierkens.dev/ptcg-decklist-tools` at an exact version.
- Add `decks.sqlite`, `deck_store`, and Tauri commands.
- Build the Decks tab: paste, preview, save, list, edit version.
- No match association, no envelope change.

Acceptance: a player can add a deck and see its rendered list, format candidates,
archetype suggestion, and unresolved warnings; data survives restart.

### Slice 2 — Active deck and match association

- Active deck control in the library and capture drawer.
- `match_decks` snapshot on save; **Deck not set** for old logs.
- Match detail display and **Change deck**.
- Undo/Trash cleanup.

Acceptance: saving a match with an active deck produces a durable snapshot;
editing the deck later does not change that match.

### Slice 3 — Portable payload and destination switch

- Add optional `deck` to the envelope.
- Per-destination **Send deck identity** switch, default off.
- Envelope and field-mapping tests.

Acceptance: with the switch off, existing destinations receive exactly the
schema-1 body; with it on, a destination receives the portable payload and can
ignore or reject it without losing the local log.

### Slice 4 — PokeDojo receiving

- Agree the hint/assignment behavior.
- Accept and display decklist, format, player archetype, and optional opponent
  archetype.
- Keep the Synced log/Game authority intact.

Acceptance: a dev PokeDojo account receives a synced log with the payload and
the player can still assign the deck.

### Slice 5 — Training Court receiving

- Revise the current device-default/empty-decklist proposal.
- Decide create/attach/ignore for `decklist_id`.
- Accept `format`, `archetype`, and optional `opp_archetype`.

Acceptance: a dev Training Court log row carries the confirmed format,
archetype, opponent archetype when present, and a decklist or an agreed hint.

## Risks

- **Decklist input friction.** Pasting a list is a new chore; the deck library
  must be fast and optional.
- **Format mismatch.** The package validates `SET-SET` windows; Training Court
  also uses `Expanded`/`GLC`. Non-window labels need a confirmed pass-through
  and receiving-side agreement.
- **Archetype drift.** Alias labels change; store the id and label, and never
  silently rewrite a confirmed snapshot.
- **Opponent archetype.** The tracker cannot detect it; null is normal. Do not
  invent a profile engine or guess from the player's deck.
- **Catalog updates.** Identity is only comparable within a catalog version.
  Re-import is a player action, not a silent merge.
- **Privacy.** A decklist is player data. Sending it is per-destination,
  explicit, and off by default.
- **Storage failure.** Deck SQLite must never become a dependency of saving a
  log or delivering one.
- **Remote semantics.** A deck identity change after delivery does not rewrite
  the remote record; document that clearly.
- **Training Court upstream.** It is public MIT; any receiving change follows
  its contribution rules and must not carry secrets or PokeDojo credentials.

## Open questions

1. Where should the deck library live: main-window tab only, or also a compact
   drawer surface? Recommendation: main window only; drawer gets active-deck
   control.
2. Should the tracker add an opponent-archetype picker in v1? Recommendation:
   no; send null and let the destination detect/edit. Add a picker only with a
   shared label source.
3. Should `deckId`/`listId` travel, or stay local? Recommendation: optional
   metadata; destinations must not require them.
4. Should per-destination support be a local switch or a destination-config
   capability? Recommendation: local switch first; a config capability later
   if destinations multiply.
5. Does PokeDojo auto-create/assign a deck from the payload, or only show it as
   a hint? Recommendation: hint only in v1.
6. Does Training Court create a decklist, attach an existing one, or store the
   text as a hint? Decision belongs in its receiving spec.
7. Who owns non-window format validation (`Expanded`, `GLC`)? The package does
   not; the tracker stores a confirmed label and the destination validates.

## Acceptance

- A saved log can have a confirmed decklist, format, player archetype, and
  optional opponent archetype snapshot.
- The raw log remains unchanged and remains the source of truth.
- A deck edit never rewrites an old match.
- With deck identity disabled, every existing destination receives the current
  schema-1 envelope.
- With deck identity enabled, PokeDojo and Training Court can consume the
  portable payload without making opaque IDs mandatory.
- `pnpm run verify`, `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, and `cargo test` pass at every slice boundary.
