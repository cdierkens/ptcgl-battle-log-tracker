# Spec: Optional decklist and editable archetype

**Status:** Approved (2026-10-26)
**Repo:** `ptcgl-battle-log-tracker`
**Base:** `refactor/unify-deck-ui` (manual deck Source selector already removed)
**Depends on:** `@dierkens.dev/ptcg-decklist-tools@0.2.0`
**Related:** [deck identity integration](2026-10-24-deck-identity-integration.md), [log sync](2026-10-12-log-sync.md)
**Receiving sides:** `pokedojo.pro/docs/specs/2026-10-12-tracker-log-sync.md`, `training-court/TRACKER-SYNC-PROPOSAL.md`

## Goal

Let a player save a deck identity without pasting a decklist: a name plus a
format and/or archetype. The deck can then be set active, snapshot onto saved
logs, and — when it has a format — sync to destinations that accept deck
identity.

The decklist remains the best identity. This feature is a lower-friction start,
not a replacement for a real list.

## Non-goals

- OCR, decklist inference, or automation of PTCG Live.
- Sprites or Pokémon artwork in the picker. Species are names in this slice.
- A species outside the 112 shipped catalog species.
- New PokeDojo or Training Court schema changes. Format-gated sync uses the
  existing envelope.
- A manual Source selector. Provenance is detected from the text; the base
  branch already removes the selector.
- A deck with only a name. A no-list deck needs a format; an archetype is optional.
- Remote edit/delete or bulk backfill.

## Decisions

| # | Decision | Why |
|---|---|---|
| D1 | Decklist optional. A deck needs a name plus either a decklist or a format. An archetype is optional; a no-list deck always ends up with a format. | The format is the sync gate; selecting a catalog archetype fills its window, and a custom archetype requires the player to pick one. |
| D2 | Format-gated sync. With a format, send `decklist: ""` + format + archetype. Without a format, omit the `deck` object. | PokeDojo already treats an empty decklist as “no decklist”; no receiving change is needed. |
| D3 | Archetype = label + 0–2 unique species slugs. Catalog entries supply `id`, `label`, `cards`, and `window`; custom entries have `id: null`. | Matches the existing `ArchetypeRef` and Training Court’s 1–2 sprite model. |
| D4 | Catalog selection fills the label and two slots. Any edit to a slot or the label makes the archetype custom. Slot edits re-derive the label only until the player edits it. | Two side-by-side pickers are the escape hatch; dirty-aware derivation preserves deliberate names. |
| D5 | Two side-by-side Pokémon inputs, names only, using the 112 shipped species. Maximum two unique species. | Same data shape as Training Court; no network or bundled art in this slice. |
| D6 | Empty list stores `rawText: ""`, `decklistText: ""`, null hashes/catalog fields, and `source: "metadata"`. | No schema migration; read paths can detect “no list” explicitly. |
| D7 | Keep `limitless` as a readable legacy source, but do not produce it for new rows. Add `metadata`. | The base branch auto-detects only PTCGL vs typed; old rows must keep loading. |
| D8 | No SQLite migration. Existing deck-list rows keep their current semantics. | Columns are already nullable or accept empty text. |
| D9 | Snapshot and match views show “No decklist” instead of parse warnings when the saved text is empty. | An empty list is a valid state, not a corrupt one. |
| D10 | No receiving-side change. PokeDojo v0.17.3+ already accepts an empty decklist and requires a format. | The tracker never sends a no-format `deck` object. |

## Player flow

1. **Add deck** opens the form.
2. Name is required.
3. Decklist is optional:
   - empty → no parse preview; continue with format/archetype.
   - non-empty but invalid → show the parser errors and block save.
   - non-empty and valid → show counts, warnings, section badges, and the
     portable preview as today.
4. Format is always visible. With no list it is required. Candidates come from
   the catalog windows (`SVI-ASC`, `TEF-30C`, `TEF-POR`); with a list they come
   from `fittingWindows(deck)` as today.
5. Archetype is always visible:
   - a catalog select grouped by window, with decklist suggestions first when a
     list is present;
   - a label input;
   - two side-by-side species inputs underneath.
6. Selecting a catalog archetype sets its label, cards, and format window.
7. Editing either species slot or the label makes the archetype custom
   (`id: null`). Slot edits derive the label from the selected species while the
   label is untouched; after a manual label edit, the label is left alone.
8. Save is allowed when the name is present and either the decklist is
   non-empty or the format is present. A no-list deck with no format shows a
   field error.

## Data model

No SQLite schema change. `DeckVersion` and `MatchDeck` keep their columns; the
meaning of the empty values is defined here.

```ts
type DeckSource = "ptcgl" | "limitless" | "typed" | "metadata";

interface DeckVersion {
  rawText: string;          // exactly what the player supplied; "" when no list
  decklistText: string;     // renderDecklist(deck, "ptcgl"); "" when no list
  formatLabel: null | string;
  archetype: null | {
    id: null | string;      // catalog id, or null for custom
    label: string;
    cards: string[];        // 0–2 unique species slugs
  };
  deckIdHash: null | string;    // null when no list
  listIdHash: null | string;    // null when no list
  hashVersion: null | number;   // null when no list
  catalogVersion: null | string;
  catalogContextId: null | string;
  source: DeckSource;
}
```

Rules:

- `rawText` and `decklistText` are both empty or both non-empty. A partial
  state is invalid.
- No list → `formatLabel` is non-null; identity hashes and catalog context are
  null; `source` is `"metadata"`.
- A list → hashes/catalog context are computed as today; `source` is `"ptcgl"`
  or `"typed"` (`"limitless"` only on legacy rows).
- `archetype` is null or has a non-empty label. `cards` are 0–2 unique species
  slugs. Label-only archetypes are allowed.
- `MatchDeck` copies the version values; a metadata-only match snapshot has
  empty list text and null hashes.

## Analysis interfaces

`analyzeDecklist` stays faithful to the package: empty input is an error. Add a
draft-level wrapper so the form can distinguish “empty” from “invalid”:

```ts
type DeckDraftAnalysis =
  | { kind: "empty" }
  | { kind: "invalid"; errors: string[] }
  | { kind: "valid"; preview: DeckPreview };

function analyzeDeckDraft(text: string): DeckDraftAnalysis;
```

- `text.trim() === ""` → `empty`.
- Otherwise run `analyzeDecklist`; map its result to `valid` or `invalid`.

The form shows Format and Archetype for every analysis kind. The decklist
preview, section badges, warnings, and portable preview render only for
`valid`.

### Catalog helpers

Derived once from the package data:

```ts
interface CatalogSpecies {
  slug: string;   // Limitless species slug, e.g. "dragapult"
  label: string;  // display name, e.g. "Dragapult"
}

const SPECIES: readonly CatalogSpecies[];          // 112 unique slugs, sorted
const ARCHETYPES: readonly CatalogArchetype[];      // 163 catalog entries
const FORMAT_WINDOWS: readonly string[];            // unique archetype.window
```

- Species labels are derived deterministically from the slug (title case,
  hyphen-separated form handling). They are display-only; the slug is stored.
- The two slot inputs are name typeaheads backed by `SPECIES`. Unknown names
  are inline field errors. Values are de-duplicated and capped at two.
- `FORMAT_WINDOWS` seeds the format datalist when there is no decklist.

### Archetype selection and label

Draft state adds the archetype pieces to the existing name/text/format draft:

```ts
interface DeckDraft {
  name: string;
  text: string;
  formatLabel: string;
  archetypeId: null | string;
  archetypeLabel: string;
  archetypeCards: string[];   // 0–2 species slugs
  labelEdited: boolean;
}
```

Behavior:

1. **Select catalog entry** → set `archetypeId`, `archetypeLabel`,
   `archetypeCards`, `labelEdited = false`; set `formatLabel` to the catalog
   entry’s `window` (overwrite).
2. **Edit a species slot** → set `archetypeId = null`; update `cards`; if
   `!labelEdited`, set the label to the selected species labels joined with
   `" / "`.
3. **Edit the label** → set `archetypeId = null` and `labelEdited = true`.
4. **Re-select a catalog entry** → reset all three and `labelEdited = false`.
5. **Clear both slots and the label** → archetype is null. If a format is
   present, the deck is format-only.
6. **Cards present, label empty on blur** → derive the label; if it remains
   empty, show a field error.
7. **Label present, cards empty** → label-only custom archetype, allowed.
8. A custom archetype does not auto-fill a format. The player must choose one
   when there is no decklist.

## Save validation

The webview validates the draft; Rust validates the persisted shape.

Webview:

- Name non-empty.
- Decklist empty or `analysis.kind === "valid"`.
- At least one of: non-empty decklist, non-empty format.
- If decklist is empty, format is required.
- Archetype: label non-empty if the archetype is present; 0–2 unique species;
  slot names must resolve to a catalog species.
- The save button stays disabled or shows field errors until these pass.

Rust `DeckStore::save`:

- Name required.
- `source` is one of `ptcgl`, `limitless`, `typed`, `metadata`.
- `metadata` is only valid with no list; `ptcgl`/`typed`/`limitless` only with
  a list.
- `rawText` and `decklistText` are both empty or both non-empty. Both empty is
  allowed only when `formatLabel` is present.
- `formatLabel` is trimmed; empty becomes null.
- Archetype, when present:
  - label is non-empty after trim;
  - cards length ≤ 2 and unique.
- A list is still allowed with no format and no archetype, as today.
- Empty decklist text no longer triggers the old “decklist is required” error
  when a format is present.

## Sync

`deck_payload` keeps the same envelope shape:

- No `formatLabel` → no `deck` object. This is unchanged and covers list-only
  decks with no format.
- `formatLabel` present → send:
  - `decklist`: `snapshot.decklist_text` (may be `""`);
  - `format`: the label;
  - `archetype`: the confirmed archetype or null;
  - `identity`: null when the snapshot has no list hashes.
- PokeDojo’s receiver already treats `decklist.trim() === ""` as “no decklist”
  and displays the format/archetype. No PokeDojo release is required.
- Training Court receiving remains its own slice. Its proposal already treats
  decklist as optional; the tracker does not assume that endpoint is live.

## Read paths

- `DeckSnapshotWarnings` checks `decklistText.trim() === ""` first and renders
  nothing (or a neutral “No decklist saved” in the parent), never a parse error.
- `MatchDeckPanel` shows the format/archetype and “No decklist” for an empty
  snapshot; the “Preview snapshot” disclosure is hidden or disabled.
- `ActiveDeckPicker`, `DeckOptions`, and `deck-label` continue to use format and
  archetype labels; no source is displayed.

## Backward compatibility

- Existing decks with lists keep their stored text, hashes, source, and
  snapshots. No migration or backfill.
- Existing `"limitless"` source rows remain readable; editing one rewrites the
  source from the current text (`ptcgl`/`typed`) or `metadata`.
- Existing match snapshots with lists render exactly as today.
- Destination behavior is unchanged when `send deck identity` is off or when a
  deck has no format.

## Testing

TypeScript:

- `deckSource("") === "metadata"`; PTCGL and typed detection unchanged.
- `analyzeDeckDraft`: empty, valid, invalid.
- Species catalog: 112 unique slugs, stable labels, no duplicate labels used
  for selection; slug/name round-trip.
- Archetype helpers: catalog select, slot edit → custom + derived label,
  manual label → no re-derivation, label-only, max two unique cards.
- Format rules: no-list candidates = catalog windows; catalog select fills the
  window; no-list save blocked without a format.
- Empty snapshot read paths do not call the parser with `""`.

Rust:

- Save no-list + format; no-list + format + archetype; no-list + archetype
  without format fails.
- Save list-only still works; list + format still works.
- `source: "metadata"` with no list passes; with a list fails.
- Partial raw/decklist text fails.
- Archetype with >2 cards or duplicate cards fails; label-only passes.
- Match snapshot copies empty list and null hashes.
- Payload with an empty list and a format includes `decklist: ""` and the
  archetype; payload without a format is omitted.
- Existing legacy `limitless` row still reads.

Project verification:

- `pnpm run verify` (typecheck, lint, test, build).
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Manual: create metadata-only decks (format-only and catalog-archetype), set
  active, save a match, check the snapshot and sync preview.

## Slices

1. **Core model and validation** — draft analysis, species/archetype helpers,
   source detection, Rust save rules, unit tests.
2. **Deck form** — optional decklist, always-visible format/archetype, catalog
   picker, two species inputs, dirty-aware label, save validation.
3. **Read paths and sync** — empty-snapshot display, payload tests, match detail.
4. **Docs and manual QA** — glossary, spec status, release notes; no receiving
   changes.

Each slice gets the repo’s `code-review` pass before the next layer.

## Acceptance

- A player can save a deck with only a name and a format, set it active, save a
  match, and see “No decklist” with the format in match detail.
- A player can select a catalog archetype without a list; the format fills from
  its window; the label and two species inputs are populated; saving produces a
  deck with an empty decklist and that archetype.
- Editing a species slot or the label makes the archetype custom; the label
  follows the dirty-aware rule; a label-only custom archetype is possible.
- A no-list deck with no format cannot be saved and explains why.
- A deck with a full list behaves exactly as before.
- With `send deck identity` on and a format, the envelope contains
  `decklist: ""`, `format`, and the archetype; PokeDojo shows the hint and no
  decklist. With no format, no `deck` object is sent.
- Existing databases, decks, and match snapshots open unchanged.
- The source dropdown remains absent on the base branch.

## Future work

- Sprite thumbnails in the species picker.
- A broader species list or catalog refresh path.
- Training Court receiving support for the same payload.
