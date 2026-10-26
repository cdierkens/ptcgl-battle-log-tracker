# Optional decklist — Implementation Plan

**Branch:** `feat/optional-decklist` (stacked on `refactor/unify-deck-ui`)
**Spec:** `docs/specs/2026-10-26-optional-decklist.md`
**Status:** Pass 2 — exact edits

Each task is TDD: add the failing test, run it, apply the code, run it again, commit.
Run the tracker commands from the repo root.

## Before Task 1

The worktree is fresh:

~~~bash
pnpm install --frozen-lockfile
pnpm build
~~~

`pnpm build` produces `dist/`, which the Rust checks need. Re-run it after UI
changes before running `cargo`.

---

## Task 1: Add draft analysis and metadata source

**Files:** `src/lib/deck-library.ts`, `src/lib/deck-library.test.ts`
**Behavior:** `analyzeDeckDraft` distinguishes blank/valid/invalid; `deckSource("")`
returns `"metadata"`.
**Depends on:** none

### Edit 1.1: Add the draft analysis and detected-source types

**File:** `src/lib/deck-library.ts`

old_str:
~~~ts
export type DeckPreviewResult =
  | { errors: string[]; ok: false; }
  | { ok: true; preview: DeckPreview };

const CATALOG = catalog;
~~~

new_str:
~~~ts
export type DeckPreviewResult =
  | { errors: string[]; ok: false; }
  | { ok: true; preview: DeckPreview };

export type DeckDraftAnalysis =
  | { kind: "empty" }
  | { errors: string[]; kind: "invalid" }
  | { kind: "valid"; preview: DeckPreview };

export type DetectedDeckSource = "metadata" | "ptcgl" | "typed";

const CATALOG = catalog;
~~~

### Edit 1.2: Add `analyzeDeckDraft` and make blank source metadata

**File:** `src/lib/deck-library.ts`

old_str:
~~~ts
/**
 * Provenance only; parsing accepts any of these shapes. Section headers plus a
 * Total Cards line identify a PTCGL/Limitless export; anything else is typed.
 */
export function deckSource(text: string): "ptcgl" | "typed" {
  const sections = /^\s*(Pokémon|Pokemon|Trainer|Energy)\s*:\s*\d+/m.test(text);
  const total = /^\s*Total Cards\s*:\s*\d+/im.test(text);
  return sections && total ? "ptcgl" : "typed";
}
~~~

new_str:
~~~ts
/** Wrap the parser for the form: blank is a valid no-list state, not an error. */
export function analyzeDeckDraft(text: string): DeckDraftAnalysis {
  if (text.trim() === "") return { kind: "empty" };
  const result = analyzeDecklist(text);
  return result.ok ? { kind: "valid", preview: result.preview } : { errors: result.errors, kind: "invalid" };
}

/**
 * Provenance only; parsing accepts any of these shapes. Section headers plus a
 * Total Cards line identify a PTCGL/Limitless export; anything else is typed.
 * Blank text means the deck has no list, not that it was typed.
 */
export function deckSource(text: string): DetectedDeckSource {
  if (text.trim() === "") return "metadata";
  const sections = /^\s*(Pokémon|Pokemon|Trainer|Energy)\s*:\s*\d+/m.test(text);
  const total = /^\s*Total Cards\s*:\s*\d+/im.test(text);
  return sections && total ? "ptcgl" : "typed";
}
~~~

### Edit 1.3: Add draft-analysis tests

**File:** `src/lib/deck-library.test.ts`

old_str:
~~~ts
import { analyzeDecklist, deckSource, identityOf, isFormatWindow } from "./deck-library";
~~~

new_str:
~~~ts
import { analyzeDeckDraft, analyzeDecklist, deckSource, identityOf, isFormatWindow } from "./deck-library";
~~~

Insert after the `analyzeDecklist` describe block:

old_str:
~~~ts
describe("identityOf", () => {
~~~

new_str:
~~~ts
describe("analyzeDeckDraft", () => {
  it("treats blank text as no list", () => {
    expect(analyzeDeckDraft("  \n")).toEqual({ kind: "empty" });
  });

  it("returns the preview for a valid list", () => {
    const result = analyzeDeckDraft(DRAGAPULT);
    expect(result.kind).toBe("valid");
    if (result.kind === "valid") expect(result.preview.decklist).toContain("Dragapult ex");
  });

  it("returns parser errors for an invalid list", () => {
    const result = analyzeDeckDraft("4");
    expect(result.kind).toBe("invalid");
    if (result.kind === "invalid") expect(result.errors.length).toBeGreaterThan(0);
  });
});

describe("identityOf", () => {
~~~

### Edit 1.4: Update the deckSource test

**File:** `src/lib/deck-library.test.ts`

old_str:
~~~ts
describe("deckSource", () => {
  it("recognises an export and treats a bare list as typed", () => {
    expect(deckSource(DRAGAPULT)).toBe("ptcgl");
    expect(deckSource("4 Ultra Ball SVI 196\n2 Basic Fire Energy MEE 10")).toBe("typed");
  });
});
~~~

new_str:
~~~ts
describe("deckSource", () => {
  it("recognises an export and treats a bare list as typed", () => {
    expect(deckSource(DRAGAPULT)).toBe("ptcgl");
    expect(deckSource("4 Ultra Ball SVI 196\n2 Basic Fire Energy MEE 10")).toBe("typed");
  });

  it("marks blank text as metadata", () => {
    expect(deckSource("  \n")).toBe("metadata");
  });
});
~~~

### Test 1: RED then GREEN

~~~bash
pnpm test -- --run src/lib/deck-library.test.ts
~~~

Expected after the edits: all `deck-library` tests pass.

### Commit 1

~~~bash
git add src/lib/deck-library.ts src/lib/deck-library.test.ts
git commit -m "feat(decks): add draft analysis and metadata source"
~~~

---

## Task 2: Add species and format catalog helpers

**Files:** `src/lib/deck-library.ts`, `src/lib/deck-library.test.ts`
**Behavior:** expose the 112 species names, slug/label lookup, label derivation, the
catalog archetypes, and the catalog windows. No UI yet.
**Depends on:** Task 1

### Edit 2.1: Import archetypes and the catalog archetype type

**File:** `src/lib/deck-library.ts`

old_str:
~~~ts
import {
  type ArchetypeSuggestion,
  deckId,
  deckView,
  fittingWindows,
  isErr,
  listId,
  type NormalizedDeck,
  normalizeDecklist,
  parseDecklist,
  parseFormatLabel,
  renderDecklist,
  suggestArchetype,
} from "@dierkens.dev/ptcg-decklist-tools";
import { catalog } from "@dierkens.dev/ptcg-decklist-tools/data";
~~~

new_str:
~~~ts
import {
  type ArchetypeSuggestion,
  type CatalogArchetype,
  deckId,
  deckView,
  fittingWindows,
  isErr,
  listId,
  type NormalizedDeck,
  normalizeDecklist,
  parseDecklist,
  parseFormatLabel,
  renderDecklist,
  suggestArchetype,
} from "@dierkens.dev/ptcg-decklist-tools";
import { archetypes, catalog } from "@dierkens.dev/ptcg-decklist-tools/data";
~~~

### Edit 2.2: Add the species type

**File:** `src/lib/deck-library.ts`

old_str:
~~~ts
export type DeckDraftAnalysis =
  | { kind: "empty" }
  | { errors: string[]; kind: "invalid" }
  | { kind: "valid"; preview: DeckPreview };

export type DetectedDeckSource = "metadata" | "ptcgl" | "typed";

const CATALOG = catalog;
~~~

new_str:
~~~ts
export type DeckDraftAnalysis =
  | { kind: "empty" }
  | { errors: string[]; kind: "invalid" }
  | { kind: "valid"; preview: DeckPreview };

export type DetectedDeckSource = "metadata" | "ptcgl" | "typed";

export interface CatalogSpecies {
  label: string;
  slug: string;
}

const CATALOG = catalog;
~~~

### Edit 2.3: Add species, archetype, and window helpers

**File:** `src/lib/deck-library.ts`

old_str:
~~~ts
const CATALOG = catalog;

/** Parse, normalise, render, and read the projections the deck editor needs. Never throws. */
export function analyzeDecklist(text: string): DeckPreviewResult {
~~~

new_str:
~~~ts
const CATALOG = catalog;

const titleCase = (value: string) =>
  value.length === 0 ? value : value.charAt(0).toUpperCase() + value.slice(1);

function speciesLabel(slug: string): string {
  const parts = slug.split("-").filter((part) => part !== "");
  const mega = parts.indexOf("mega");
  if (mega >= 0) {
    return ["mega", ...parts.slice(0, mega), ...parts.slice(mega + 1)].map(titleCase).join(" ");
  }
  return parts.map(titleCase).join(" ");
}

const SPECIES_BY_SLUG = new Map<string, CatalogSpecies>();
for (const card of CATALOG.cards) {
  if (card.species === null || SPECIES_BY_SLUG.has(card.species)) continue;
  SPECIES_BY_SLUG.set(card.species, { label: speciesLabel(card.species), slug: card.species });
}

export const SPECIES: readonly CatalogSpecies[] = [...SPECIES_BY_SLUG.values()].sort((left, right) =>
  left.label.localeCompare(right.label),
);

const SPECIES_BY_LABEL = new Map(SPECIES.map((species) => [species.label.toLowerCase(), species]));

export const ARCHETYPES: readonly CatalogArchetype[] = archetypes;
export const FORMAT_WINDOWS: readonly string[] = [...new Set(archetypes.map((archetype) => archetype.window))].sort();

export function findSpecies(value: string): CatalogSpecies | undefined {
  const key = value.trim().toLowerCase();
  return SPECIES_BY_SLUG.get(key) ?? SPECIES_BY_LABEL.get(key);
}

export function speciesLabelFor(slug: string): string {
  return SPECIES_BY_SLUG.get(slug)?.label ?? speciesLabel(slug);
}

export function deriveArchetypeLabel(values: readonly string[]): string {
  const labels = values
    .map((value) => value.trim())
    .filter((value) => value !== "")
    .map((value) => findSpecies(value)?.label ?? value);
  return [...new Set(labels)].join(" / ");
}

/** Parse, normalise, render, and read the projections the deck editor needs. Never throws. */
export function analyzeDecklist(text: string): DeckPreviewResult {
~~~

### Edit 2.4: Add helper tests

**File:** `src/lib/deck-library.test.ts`

old_str:
~~~ts
import { analyzeDeckDraft, analyzeDecklist, deckSource, identityOf, isFormatWindow } from "./deck-library";
~~~

new_str:
~~~ts
import {
  analyzeDeckDraft,
  analyzeDecklist,
  deckSource,
  deriveArchetypeLabel,
  findSpecies,
  FORMAT_WINDOWS,
  identityOf,
  isFormatWindow,
  SPECIES,
  speciesLabelFor,
} from "./deck-library";
~~~

Append after the `deckSource` describe:

old_str:
~~~ts
  it("marks blank text as metadata", () => {
    expect(deckSource("  \n")).toBe("metadata");
  });
});
~~~

new_str:
~~~ts
  it("marks blank text as metadata", () => {
    expect(deckSource("  \n")).toBe("metadata");
  });
});

describe("species catalog", () => {
  it("exposes unique labels and slugs", () => {
    expect(SPECIES.length).toBeGreaterThan(0);
    expect(new Set(SPECIES.map((species) => species.slug)).size).toBe(SPECIES.length);
    expect(new Set(SPECIES.map((species) => species.label)).size).toBe(SPECIES.length);
  });

  it("round-trips labels and slugs", () => {
    expect(findSpecies("Dragapult")?.slug).toBe("dragapult");
    expect(findSpecies("dragapult")?.label).toBe("Dragapult");
    expect(speciesLabelFor("excadrill-mega")).toBe("Mega Excadrill");
  });

  it("derives a label from names", () => {
    expect(deriveArchetypeLabel(["Dragapult", "Blaziken"])).toBe("Dragapult / Blaziken");
    expect(deriveArchetypeLabel(["Dragapult", "Dragapult"])).toBe("Dragapult");
    expect(deriveArchetypeLabel([])).toBe("");
  });

  it("exposes the catalog windows", () => {
    expect(FORMAT_WINDOWS).toContain("TEF-30C");
  });
});
~~~

### Test 2: RED then GREEN

~~~bash
pnpm test -- --run src/lib/deck-library.test.ts
~~~

Expected: all `deck-library` tests pass.

### Commit 2

~~~bash
git add src/lib/deck-library.ts src/lib/deck-library.test.ts
git commit -m "feat(decks): expose archetype and species catalog helpers"
~~~

---

## Task 3: Add archetype draft state and helpers

**Files:** `src/lib/deck-draft.ts`, new `src/lib/deck-draft.test.ts`
**Behavior:** the editor draft carries the catalog id, label, and two raw species
slots; catalog selection, slot edits, label edits, clearing, and save validation
are pure functions.
**Depends on:** Task 2

### Edit 3.1: Replace `deck-draft.ts`

**File:** `src/lib/deck-draft.ts`

old_str:
~~~ts
import type { DeckPreview } from "./deck-library";
import type { Deck, SaveDeckInput } from "./tauri";

export interface DeckDraft {
  archetypeKey: string;
  formatLabel: string;
  name: string;
  text: string;
}

export const BLANK_DECK_DRAFT: DeckDraft = {
  archetypeKey: "",
  formatLabel: "",
  name: "",
  text: "",
};

/** Resolve the confirmed archetype from the editor's selection. */
export function selectedArchetype(
  preview: DeckPreview,
  key: string,
  editing: Deck | null,
): SaveDeckInput["archetype"] {
  const suggestion = preview.archetypeSuggestions.find((candidate) => candidate.id === key);
  if (suggestion !== undefined) {
    return { cards: [...suggestion.cards], id: suggestion.id, label: suggestion.label };
  }
  const stored = editing?.currentVersion.archetype ?? null;
  if (key !== "" && stored !== null && (stored.id ?? stored.label) === key) return stored;
  return null;
}
~~~

new_str:
~~~ts
import type { ArchetypeSuggestion, CatalogArchetype } from "@dierkens.dev/ptcg-decklist-tools";

import type { CatalogSpecies, DeckDraftAnalysis } from "./deck-library";
import type { ArchetypeRef, Deck } from "./tauri";

import { deriveArchetypeLabel, findSpecies, speciesLabelFor } from "./deck-library";

export interface DeckDraft {
  archetypeId: null | string;
  archetypeLabel: string;
  archetypeSpecies: string[];
  formatLabel: string;
  labelEdited: boolean;
  name: string;
  text: string;
}

export interface DeckDraftErrors {
  archetype: null | string;
  decklist: null | string;
  format: null | string;
  name: null | string;
}

export const BLANK_DECK_DRAFT: DeckDraft = {
  archetypeId: null,
  archetypeLabel: "",
  archetypeSpecies: ["", ""],
  formatLabel: "",
  labelEdited: false,
  name: "",
  text: "",
};

export function applyCatalogArchetype(draft: DeckDraft, archetype: CatalogArchetype): DeckDraft {
  const cards = archetype.cards;
  return {
    ...draft,
    archetypeId: archetype.id,
    archetypeLabel: archetype.label,
    archetypeSpecies: [
      cards[0] === undefined ? "" : speciesLabelFor(cards[0]),
      cards[1] === undefined ? "" : speciesLabelFor(cards[1]),
    ],
    formatLabel: archetype.window,
    labelEdited: false,
  };
}

export function applySuggestedArchetype(draft: DeckDraft, suggestion: ArchetypeSuggestion): DeckDraft {
  const cards = suggestion.cards;
  return {
    ...draft,
    archetypeId: suggestion.id,
    archetypeLabel: suggestion.label,
    archetypeSpecies: [
      cards[0] === undefined ? "" : speciesLabelFor(cards[0]),
      cards[1] === undefined ? "" : speciesLabelFor(cards[1]),
    ],
    labelEdited: false,
  };
}

export function archetypeFromDraft(draft: DeckDraft): ArchetypeRef | null {
  const cards = draft.archetypeSpecies
    .map((value) => (value.trim() === "" ? null : findSpecies(value)))
    .filter((species): species is CatalogSpecies => species !== null && species !== undefined)
    .map((species) => species.slug);
  const label = draft.archetypeLabel.trim();
  if (cards.length === 0 && label === "") return null;
  return { cards, id: draft.archetypeId, label };
}

export function clearArchetype(draft: DeckDraft): DeckDraft {
  return {
    ...draft,
    archetypeId: null,
    archetypeLabel: "",
    archetypeSpecies: ["", ""],
    labelEdited: false,
  };
}

export function deckDraftErrors(analysis: DeckDraftAnalysis, draft: DeckDraft): DeckDraftErrors {
  let archetype: null | string = null;
  const seen = new Set<string>();
  for (const value of draft.archetypeSpecies) {
    const trimmed = value.trim();
    if (trimmed === "") continue;
    const species = findSpecies(trimmed);
    if (species === undefined) {
      archetype = "Choose a Pokémon from the list.";
      break;
    }
    if (seen.has(species.slug)) {
      archetype = "Choose two different Pokémon.";
      break;
    }
    seen.add(species.slug);
  }
  if (
    archetype === null &&
    draft.archetypeLabel.trim() === "" &&
    draft.archetypeSpecies.some((value) => value.trim() !== "")
  ) {
    archetype = "Give the archetype a label.";
  }
  return {
    archetype,
    decklist: analysis.kind === "invalid" ? analysis.errors.join(" ") : null,
    format: analysis.kind === "empty" && draft.formatLabel.trim() === "" ? "Choose a format, or add a decklist." : null,
    name: draft.name.trim() === "" ? "Give the deck a name." : null,
  };
}

export function draftFromDeck(deck: Deck): DeckDraft {
  const version = deck.currentVersion;
  const archetype = version.archetype;
  const cards = archetype?.cards ?? [];
  return {
    archetypeId: archetype?.id ?? null,
    archetypeLabel: archetype?.label ?? "",
    archetypeSpecies: [
      cards[0] === undefined ? "" : speciesLabelFor(cards[0]),
      cards[1] === undefined ? "" : speciesLabelFor(cards[1]),
    ],
    formatLabel: version.formatLabel ?? "",
    labelEdited: archetype !== null && archetype.id === null,
    name: deck.name,
    text: version.rawText,
  };
}

export function setArchetypeLabel(draft: DeckDraft, label: string): DeckDraft {
  return {
    ...draft,
    archetypeId: null,
    archetypeLabel: label,
    labelEdited: true,
  };
}

export function setArchetypeSlot(draft: DeckDraft, index: number, value: string): DeckDraft {
  const slots = [draft.archetypeSpecies[0] ?? "", draft.archetypeSpecies[1] ?? ""];
  slots[index] = value;
  const species = slots.filter((slot) => slot.trim() !== "");
  return {
    ...draft,
    archetypeId: null,
    archetypeLabel: draft.labelEdited ? draft.archetypeLabel : deriveArchetypeLabel(species),
    archetypeSpecies: slots,
  };
}
~~~

### Edit 3.2: Add `deck-draft.test.ts`

**File:** `src/lib/deck-draft.test.ts` (new)

~~~ts
import { describe, expect, it } from "vitest";

import type { DeckDraftAnalysis } from "./deck-library";
import type { Deck } from "./tauri";

import {
  applyCatalogArchetype,
  archetypeFromDraft,
  BLANK_DECK_DRAFT,
  clearArchetype,
  deckDraftErrors,
  draftFromDeck,
  setArchetypeLabel,
  setArchetypeSlot,
} from "./deck-draft";
import { ARCHETYPES, speciesLabelFor } from "./deck-library";

const DRAGAPULT = ARCHETYPES.find((archetype) => archetype.label === "Dragapult ex");
if (DRAGAPULT === undefined) throw new Error("catalog is missing Dragapult ex");

const deck: Deck = {
  archivedAt: null,
  createdAt: 1,
  currentVersion: {
    archetype: { cards: ["dragapult"], id: DRAGAPULT.id, label: DRAGAPULT.label },
    catalogContextId: null,
    catalogVersion: "2026-10-23",
    createdAt: 1,
    deckIdHash: "sha256:deck",
    decklistText: "Total Cards: 4\n",
    formatLabel: "TEF-30C",
    hashVersion: 1,
    id: "version-1",
    listIdHash: "sha256:list",
    rawText: "4 Ultra Ball SVI 196",
    source: "ptcgl",
  },
  id: "deck-1",
  name: "Dragapult",
  updatedAt: 1,
};

describe("draftFromDeck", () => {
  it("maps a catalog archetype into editable species names", () => {
    const draft = draftFromDeck(deck);
    expect(draft.archetypeId).toBe(DRAGAPULT.id);
    expect(draft.archetypeSpecies).toEqual(DRAGAPULT.cards.map(speciesLabelFor));
    expect(draft.formatLabel).toBe("TEF-30C");
    expect(draft.labelEdited).toBe(false);
  });

  it("marks a stored custom archetype as label-edited", () => {
    const customDeck: Deck = {
      ...deck,
      currentVersion: {
        ...deck.currentVersion,
        archetype: { cards: ["dragapult"], id: null, label: "My Dragapult" },
      },
    };
    expect(draftFromDeck(customDeck).labelEdited).toBe(true);
  });
});

describe("archetype draft transitions", () => {
  it("fills label, cards, and format from a catalog archetype", () => {
    const draft = applyCatalogArchetype(BLANK_DECK_DRAFT, DRAGAPULT);
    expect(draft.archetypeId).toBe(DRAGAPULT.id);
    expect(draft.archetypeLabel).toBe(DRAGAPULT.label);
    expect(draft.archetypeSpecies).toEqual(DRAGAPULT.cards.map(speciesLabelFor));
    expect(draft.formatLabel).toBe(DRAGAPULT.window);
    expect(draft.labelEdited).toBe(false);
  });

  it("makes a slot edit custom and derives the label", () => {
    const catalogDraft = applyCatalogArchetype(BLANK_DECK_DRAFT, DRAGAPULT);
    const edited = setArchetypeSlot(catalogDraft, 1, "Dusknoir");
    expect(edited.archetypeId).toBeNull();
    expect(edited.archetypeSpecies).toEqual(["Dragapult", "Dusknoir"]);
    expect(edited.archetypeLabel).toBe("Dragapult / Dusknoir");
  });

  it("keeps a manually edited label on later slot edits", () => {
    const catalogDraft = applyCatalogArchetype(BLANK_DECK_DRAFT, DRAGAPULT);
    const labeled = setArchetypeLabel(catalogDraft, "My Dragapult");
    const edited = setArchetypeSlot(labeled, 1, "Dusknoir");
    expect(edited.archetypeId).toBeNull();
    expect(edited.archetypeLabel).toBe("My Dragapult");
  });

  it("clears the archetype without touching the rest of the draft", () => {
    const catalogDraft = applyCatalogArchetype(
      { ...BLANK_DECK_DRAFT, formatLabel: "SVI-ASC", name: "Deck" },
      DRAGAPULT,
    );
    const cleared = clearArchetype(catalogDraft);
    expect(cleared.archetypeId).toBeNull();
    expect(cleared.archetypeLabel).toBe("");
    expect(cleared.archetypeSpecies).toEqual(["", ""]);
    expect(cleared.formatLabel).toBe(DRAGAPULT.window);
    expect(cleared.name).toBe("Deck");
  });

  it("builds an ArchetypeRef with species slugs", () => {
    const draft = {
      ...BLANK_DECK_DRAFT,
      archetypeLabel: "Custom",
      archetypeSpecies: ["Dragapult", "Dusknoir"],
    };
    expect(archetypeFromDraft(draft)).toEqual({
      cards: ["dragapult", "dusknoir"],
      id: null,
      label: "Custom",
    });
  });

  it("returns null for an empty archetype", () => {
    expect(archetypeFromDraft(BLANK_DECK_DRAFT)).toBeNull();
  });
});

describe("deckDraftErrors", () => {
  const emptyAnalysis: DeckDraftAnalysis = { kind: "empty" };
  const formatDraft = { ...BLANK_DECK_DRAFT, formatLabel: "TEF-30C", name: "Deck" };

  it("requires a name and a format for a no-list deck", () => {
    expect(deckDraftErrors(emptyAnalysis, BLANK_DECK_DRAFT)).toMatchObject({
      format: expect.any(String),
      name: expect.any(String),
    });
    expect(deckDraftErrors(emptyAnalysis, formatDraft)).toEqual({
      archetype: null,
      decklist: null,
      format: null,
      name: null,
    });
  });

  it("surfaces parser errors", () => {
    const invalid: DeckDraftAnalysis = { errors: ["bad line"], kind: "invalid" };
    expect(deckDraftErrors(invalid, formatDraft).decklist).toBe("bad line");
  });

  it("rejects unknown and duplicate species", () => {
    const unknown = { ...formatDraft, archetypeLabel: "X", archetypeSpecies: ["Not a Pokémon", ""] };
    expect(deckDraftErrors(emptyAnalysis, unknown).archetype).toContain("Choose a Pokémon");
    const duplicate = { ...formatDraft, archetypeLabel: "X", archetypeSpecies: ["Dragapult", "Dragapult"] };
    expect(deckDraftErrors(emptyAnalysis, duplicate).archetype).toContain("two different");
  });

  it("requires a label when species are present", () => {
    const draft = { ...formatDraft, archetypeSpecies: ["Dragapult", ""] };
    expect(deckDraftErrors(emptyAnalysis, draft).archetype).toContain("label");
  });
});
~~~

### Test 3: RED then GREEN

~~~bash
pnpm test -- --run src/lib/deck-draft.test.ts
~~~

Expected: all `deck-draft` tests pass.

### Commit 3

~~~bash
git add src/lib/deck-draft.ts src/lib/deck-draft.test.ts
git commit -m "feat(decks): add archetype draft state and validation"
~~~

---

## Task 4: Make the deck form support metadata-only decks

**Files:** `src/components/DeckForm.tsx`, `src/components/DecksTab.tsx`
**Behavior:** the form always shows Format and Archetype; decklist is optional; the
catalog select, label, and two species inputs drive the draft; DecksTab saves
empty lists with null hashes and `source: "metadata"`.
**Depends on:** Tasks 1–3

### Edit 4.1: Replace `DeckForm.tsx`

**File:** `src/components/DeckForm.tsx`

old_str:
~~~tsx
import { LoadingButton } from "@/components/LoadingButton";
import { StatusAlert } from "@/components/StatusAlert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

import type { DeckDraft } from "../lib/deck-draft";
import type { Deck } from "../lib/tauri";

import { analyzeDecklist, type DeckPreview, isFormatWindow } from "../lib/deck-library";
import { SELECT_CLASS } from "../lib/form-styles";

interface DeckFormProps {
  analysis: ReturnType<typeof analyzeDecklist>;
  draft: DeckDraft;
  editing: Deck | null;
  onCancel: () => void;
  onChange: (patch: Partial<DeckDraft>) => void;
  onSave: () => void;
  preview: DeckPreview | null;
  saving: boolean;
}

export function DeckForm({
  analysis,
  draft,
  editing,
  onCancel,
  onChange,
  onSave,
  preview,
  saving,
}: DeckFormProps) {
  const suggestions = preview?.archetypeSuggestions ?? [];
  const storedArchetype = editing?.currentVersion.archetype ?? null;
  const storedOption = storedArchetype !== null && !suggestions.some((suggestion) => suggestion.id === storedArchetype.id)
    ? storedArchetype
    : null;
  const shownSuggestion = suggestions.find((suggestion) => suggestion.id === draft.archetypeKey)
    ?? (draft.archetypeKey === "" ? suggestions[0] : undefined);
  const storedSelected =
    storedOption !== null && (storedOption.id ?? storedOption.label) === draft.archetypeKey;

  return (
    <div className="space-y-3 rounded-md border border-border p-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">{editing === null ? "Add a deck" : `Edit ${editing.name}`}</h3>
        <Button onClick={onCancel} size="sm" variant="ghost">Cancel</Button>
      </div>

      <Field>
        <FieldLabel htmlFor="deck-name">Name</FieldLabel>
        <Input id="deck-name" onChange={(event) => onChange({ name: event.target.value })} placeholder="League deck" value={draft.name} />
      </Field>

      <Field data-invalid={!analysis.ok || undefined}>
        <FieldLabel htmlFor="decklist">Decklist</FieldLabel>
        <Textarea
          className="min-h-40 font-mono text-xs"
          id="decklist"
          onChange={(event) => onChange({ text: event.target.value })}
          placeholder="Paste a PTCGL export, a Limitless list, or type the list"
          value={draft.text}
        />
        <FieldDescription>Parsed locally. The original text and a portable PTCGL render are stored.</FieldDescription>
        <FieldError>{analysis.ok ? null : analysis.errors.join(" ")}</FieldError>
      </Field>

      {preview !== null ? (
        <>
          <div className="flex flex-wrap gap-2 text-xs">
            {preview.sections.map((section) => (
              <Badge key={section.section} variant="secondary">{section.section}: {section.count}</Badge>
            ))}
            {preview.unresolved > 0 ? <Badge variant="destructive">{preview.unresolved} unresolved</Badge> : null}
          </div>
          {preview.warnings.length > 0 ? (
            <StatusAlert tone="info">{preview.warnings.join(" ")}</StatusAlert>
          ) : null}

          <Field>
            <FieldLabel htmlFor="deck-format">Format</FieldLabel>
            <Input
              id="deck-format"
              list="deck-format-candidates"
              onChange={(event) => onChange({ formatLabel: event.target.value })}
              placeholder="TEF-30C"
              value={draft.formatLabel}
            />
            <datalist id="deck-format-candidates">
              {preview.formatCandidates.map((candidate) => <option key={candidate} value={candidate} />)}
            </datalist>
            <FieldDescription>
              {draft.formatLabel.trim() === ""
                ? "Pick a fitting window or type a confirmed label."
                : isFormatWindow(draft.formatLabel)
                  ? "A set window."
                  : "Not a set window. It will be stored as a confirmed label."}
            </FieldDescription>
          </Field>

          <Field>
            <FieldLabel htmlFor="deck-archetype">Archetype</FieldLabel>
            <select
              className={cn(SELECT_CLASS, "h-10 w-full")}
              id="deck-archetype"
              onChange={(event) => onChange({ archetypeKey: event.target.value })}
              value={draft.archetypeKey}
            >
              <option value="">No archetype confirmed</option>
              {storedOption !== null ? <option value={storedOption.id ?? storedOption.label}>{storedOption.label}</option> : null}
              {suggestions.map((suggestion) => (
                <option key={suggestion.id} value={suggestion.id}>
                  {suggestion.label} ({Math.round(suggestion.confidence * 100)}%)
                </option>
              ))}
            </select>
            <FieldDescription>
              {shownSuggestion !== undefined
                ? `${evidenceText(shownSuggestion)} ${shownSuggestion.reasons.join(" ")}`
                : storedSelected && storedOption !== null
                  ? `Confirmed archetype: ${storedOption.label}.`
                  : "No suggestion yet."}
            </FieldDescription>
          </Field>

          <details className="text-xs text-muted-foreground">
            <summary className="cursor-pointer">Preview portable decklist</summary>
            <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border p-2 font-mono">{preview.decklist}</pre>
          </details>
        </>
      ) : null}

      <div className="flex justify-end gap-2">
        <Button onClick={onCancel} size="sm" variant="secondary">Cancel</Button>
        <LoadingButton loading={saving} onClick={onSave} size="sm">Save deck</LoadingButton>
      </div>
    </div>
  );
}

function evidenceText(suggestion: DeckPreview["archetypeSuggestions"][number]): string {
  if (suggestion.evidence.kind === "alias") {
    const matched = suggestion.evidence.matched.map((match) => `${match.count}× ${match.name}`).join(", ");
    return `Matched ${matched} in ${suggestion.evidence.windows.join(", ")}.`;
  }
  return `Line ${suggestion.evidence.line}: ${suggestion.evidence.copies} cards across ${suggestion.evidence.stages} stage(s), ${suggestion.evidence.traits.join(", ")}.`;
}
~~~

new_str:
~~~tsx
import { LoadingButton } from "@/components/LoadingButton";
import { StatusAlert } from "@/components/StatusAlert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

import type { DeckDraft, DeckDraftErrors } from "../lib/deck-draft";
import type { DeckDraftAnalysis, DeckPreview } from "../lib/deck-library";
import type { Deck } from "../lib/tauri";

import {
  applyCatalogArchetype,
  applySuggestedArchetype,
  clearArchetype,
  setArchetypeLabel,
  setArchetypeSlot,
} from "../lib/deck-draft";
import { ARCHETYPES, FORMAT_WINDOWS, isFormatWindow, SPECIES } from "../lib/deck-library";
import { SELECT_CLASS } from "../lib/form-styles";

const CUSTOM_ARCHETYPE = "__custom__";

interface DeckFormProps {
  analysis: DeckDraftAnalysis;
  draft: DeckDraft;
  editing: Deck | null;
  errors: DeckDraftErrors;
  onCancel: () => void;
  onChange: (patch: Partial<DeckDraft>) => void;
  onSave: () => void;
  saving: boolean;
  showErrors: boolean;
}

export function DeckForm({
  analysis,
  draft,
  editing,
  errors,
  onCancel,
  onChange,
  onSave,
  saving,
  showErrors,
}: DeckFormProps) {
  const preview = analysis.kind === "valid" ? analysis.preview : null;
  const suggestions = preview?.archetypeSuggestions ?? [];
  const shownSuggestion = suggestions.find((suggestion) => suggestion.id === draft.archetypeId);
  const hasArchetype =
    draft.archetypeId !== null ||
    draft.archetypeLabel.trim() !== "" ||
    draft.archetypeSpecies.some((value) => value.trim() !== "");
  const selectedCatalogId =
    draft.archetypeId !== null && ARCHETYPES.some((archetype) => archetype.id === draft.archetypeId)
      ? draft.archetypeId
      : hasArchetype
        ? CUSTOM_ARCHETYPE
        : "";
  const suggestionIds = new Set(suggestions.map((suggestion) => suggestion.id));
  const catalogOptions = ARCHETYPES.filter((archetype) => !suggestionIds.has(archetype.id));
  const formatCandidates = preview?.formatCandidates ?? FORMAT_WINDOWS;
  const formatValue = draft.formatLabel.trim();
  const formatDescription =
    formatValue === ""
      ? "Pick a fitting window or type a confirmed label."
      : isFormatWindow(formatValue)
        ? "A set window."
        : "Not a set window. It will be stored as a confirmed label.";

  const selectArchetype = (value: string) => {
    if (value === "") {
      onChange(clearArchetype(draft));
      return;
    }
    if (value === CUSTOM_ARCHETYPE) return;
    const catalog = ARCHETYPES.find((archetype) => archetype.id === value);
    if (catalog !== undefined) {
      onChange(applyCatalogArchetype(draft, catalog));
      return;
    }
    const suggestion = suggestions.find((candidate) => candidate.id === value);
    if (suggestion !== undefined) onChange(applySuggestedArchetype(draft, suggestion));
  };

  return (
    <div className="space-y-3 rounded-md border border-border p-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">{editing === null ? "Add a deck" : `Edit ${editing.name}`}</h3>
        <Button onClick={onCancel} size="sm" variant="ghost">Cancel</Button>
      </div>

      <Field data-invalid={(showErrors && errors.name !== null) || undefined}>
        <FieldLabel htmlFor="deck-name">Name</FieldLabel>
        <Input id="deck-name" onChange={(event) => onChange({ name: event.target.value })} placeholder="League deck" value={draft.name} />
        <FieldError>{showErrors ? errors.name : null}</FieldError>
      </Field>

      <Field data-invalid={errors.decklist !== null || undefined}>
        <FieldLabel htmlFor="decklist">Decklist</FieldLabel>
        <Textarea
          className="min-h-40 font-mono text-xs"
          id="decklist"
          onChange={(event) => onChange({ text: event.target.value })}
          placeholder="Optional: paste a PTCGL export, a Limitless list, or type the list"
          value={draft.text}
        />
        <FieldDescription>
          Optional. Parsed locally. The original text and a portable PTCGL render are stored when present.
        </FieldDescription>
        <FieldError>{errors.decklist}</FieldError>
      </Field>

      {preview !== null ? (
        <>
          <div className="flex flex-wrap gap-2 text-xs">
            {preview.sections.map((section) => (
              <Badge key={section.section} variant="secondary">{section.section}: {section.count}</Badge>
            ))}
            {preview.unresolved > 0 ? <Badge variant="destructive">{preview.unresolved} unresolved</Badge> : null}
          </div>
          {preview.warnings.length > 0 ? (
            <StatusAlert tone="info">{preview.warnings.join(" ")}</StatusAlert>
          ) : null}
        </>
      ) : null}

      <Field data-invalid={(showErrors && errors.format !== null) || undefined}>
        <FieldLabel htmlFor="deck-format">Format</FieldLabel>
        <Input
          id="deck-format"
          list="deck-format-candidates"
          onChange={(event) => onChange({ formatLabel: event.target.value })}
          placeholder="TEF-30C"
          value={draft.formatLabel}
        />
        <datalist id="deck-format-candidates">
          {formatCandidates.map((candidate) => <option key={candidate} value={candidate} />)}
        </datalist>
        <FieldDescription>{formatDescription}</FieldDescription>
        <FieldError>{showErrors ? errors.format : null}</FieldError>
      </Field>

      <Field>
        <FieldLabel htmlFor="deck-archetype">Archetype</FieldLabel>
        <select
          className={cn(SELECT_CLASS, "h-10 w-full")}
          id="deck-archetype"
          onChange={(event) => selectArchetype(event.target.value)}
          value={selectedCatalogId}
        >
          <option value="">No archetype</option>
          {selectedCatalogId === CUSTOM_ARCHETYPE ? <option value={CUSTOM_ARCHETYPE}>Custom archetype</option> : null}
          {suggestions.length > 0 ? (
            <optgroup label="Suggested for this decklist">
              {suggestions.map((suggestion) => (
                <option key={suggestion.id} value={suggestion.id}>
                  {suggestion.label} ({Math.round(suggestion.confidence * 100)}%)
                </option>
              ))}
            </optgroup>
          ) : null}
          <optgroup label="All archetypes">
            {catalogOptions.map((archetype) => (
              <option key={archetype.id} value={archetype.id}>
                {archetype.label} · {archetype.window}
              </option>
            ))}
          </optgroup>
        </select>
        <FieldDescription>
          Select a catalog archetype, or enter up to two Pokémon below.
          {shownSuggestion === undefined ? "" : ` ${evidenceText(shownSuggestion)} ${shownSuggestion.reasons.join(" ")}`}
        </FieldDescription>
      </Field>

      <Field>
        <FieldLabel htmlFor="deck-archetype-label">Archetype label</FieldLabel>
        <Input
          id="deck-archetype-label"
          onChange={(event) => onChange(setArchetypeLabel(draft, event.target.value))}
          placeholder="Dragapult / Dusknoir"
          value={draft.archetypeLabel}
        />
      </Field>

      <div className="grid grid-cols-2 gap-2">
        {[0, 1].map((index) => (
          <Field key={index}>
            <FieldLabel htmlFor={`deck-species-${index}`}>Pokémon {index + 1}</FieldLabel>
            <Input
              id={`deck-species-${index}`}
              list="deck-species-candidates"
              onChange={(event) => onChange(setArchetypeSlot(draft, index, event.target.value))}
              placeholder="Search a Pokémon"
              value={draft.archetypeSpecies[index] ?? ""}
            />
          </Field>
        ))}
      </div>
      <datalist id="deck-species-candidates">
        {SPECIES.map((species) => <option key={species.slug} value={species.label} />)}
      </datalist>
      {showErrors && errors.archetype !== null ? <FieldError>{errors.archetype}</FieldError> : null}

      {preview !== null ? (
        <details className="text-xs text-muted-foreground">
          <summary className="cursor-pointer">Preview portable decklist</summary>
          <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border p-2 font-mono">{preview.decklist}</pre>
        </details>
      ) : null}

      <div className="flex justify-end gap-2">
        <Button onClick={onCancel} size="sm" variant="secondary">Cancel</Button>
        <LoadingButton loading={saving} onClick={onSave} size="sm">Save deck</LoadingButton>
      </div>
    </div>
  );
}

function evidenceText(suggestion: DeckPreview["archetypeSuggestions"][number]): string {
  if (suggestion.evidence.kind === "alias") {
    const matched = suggestion.evidence.matched.map((match) => `${match.count}× ${match.name}`).join(", ");
    return `Matched ${matched} in ${suggestion.evidence.windows.join(", ")}.`;
  }
  return `Line ${suggestion.evidence.line}: ${suggestion.evidence.copies} cards across ${suggestion.evidence.stages} stage(s), ${suggestion.evidence.traits.join(", ")}.`;
}
~~~

### Edit 4.2: Update `DecksTab` imports and analysis

**File:** `src/components/DecksTab.tsx`

old_str:
~~~tsx
import { BLANK_DECK_DRAFT, type DeckDraft, selectedArchetype } from "../lib/deck-draft";
import { analyzeDecklist, deckSource, identityOf } from "../lib/deck-library";
~~~

new_str:
~~~tsx
import {
  archetypeFromDraft,
  BLANK_DECK_DRAFT,
  type DeckDraft,
  deckDraftErrors,
  draftFromDeck,
} from "../lib/deck-draft";
import { analyzeDeckDraft, deckSource, identityOf } from "../lib/deck-library";
~~~

old_str:
~~~tsx
  const analysis = useMemo(() => analyzeDecklist(draft.text), [draft.text]);
  const preview = analysis.ok ? analysis.preview : null;
~~~

new_str:
~~~tsx
  const analysis = useMemo(() => analyzeDeckDraft(draft.text), [draft.text]);
  const errors = deckDraftErrors(analysis, draft);
~~~

### Edit 4.2a: Add the form-error visibility state

**File:** `src/components/DecksTab.tsx`

old_str:
~~~tsx
  const [formError, setFormError] = useState<null | string>(null);
  const [pendingArchive, setPendingArchive] = useState<Deck | null>(null);
~~~

new_str:
~~~tsx
  const [formError, setFormError] = useState<null | string>(null);
  const [pendingArchive, setPendingArchive] = useState<Deck | null>(null);
  const [showErrors, setShowErrors] = useState(false);
~~~

old_str:
~~~tsx
  const reset = () => {
    setEditing(null);
    setDraft(BLANK_DECK_DRAFT);
    setFormError(null);
    setShowForm(false);
  };
~~~

new_str:
~~~tsx
  const reset = () => {
    setEditing(null);
    setDraft(BLANK_DECK_DRAFT);
    setFormError(null);
    setShowErrors(false);
    setShowForm(false);
  };
~~~

### Edit 4.3: Use `draftFromDeck` when editing

**File:** `src/components/DecksTab.tsx`

old_str:
~~~tsx
  const startEdit = (deck: Deck) => {
    const version = deck.currentVersion;
    setEditing(deck);
    setDraft({
      archetypeKey: version.archetype?.id ?? version.archetype?.label ?? "",
      formatLabel: version.formatLabel ?? "",
      name: deck.name,
      text: version.rawText,
    });
    setFormError(null);
    setShowForm(true);
  };
~~~

new_str:
~~~tsx
  const startEdit = (deck: Deck) => {
    setEditing(deck);
    setDraft(draftFromDeck(deck));
    setFormError(null);
    setShowErrors(false);
    setShowForm(true);
  };
~~~

### Edit 4.4: Replace the save path

**File:** `src/components/DecksTab.tsx`

old_str:
~~~tsx
  const save = async () => {
    if (draft.name.trim() === "") {
      setFormError("Give the deck a name.");
      return;
    }
    if (!analysis.ok) {
      setFormError(analysis.errors.join(" "));
      return;
    }

    try {
      const selected = selectedArchetype(analysis.preview, draft.archetypeKey, editing);
      const identity = await identityOf(analysis.preview.deck);
      const input: SaveDeckInput = {
        archetype: selected,
        catalogContextId: identity.catalogContextId,
        catalogVersion: identity.catalogVersion,
        deckIdHash: identity.deckId,
        decklistText: analysis.preview.decklist,
        formatLabel: draft.formatLabel.trim() === "" ? null : draft.formatLabel.trim(),
        hashVersion: identity.hashVersion,
        id: editing?.id,
        listIdHash: identity.listId,
        name: draft.name.trim(),
        rawText: draft.text,
        source: deckSource(draft.text),
      };

      const saved = await decks.save(input);
      if (saved !== null) reset();
    } catch (error) {
      setFormError(String(error));
    }
  };
~~~

new_str:
~~~tsx
  const save = async () => {
    setShowErrors(true);
    const currentErrors = deckDraftErrors(analysis, draft);
    const firstError =
      currentErrors.name ?? currentErrors.decklist ?? currentErrors.format ?? currentErrors.archetype;
    if (firstError !== null) {
      setFormError(firstError);
      return;
    }

    try {
      const identity = analysis.kind === "valid" ? await identityOf(analysis.preview.deck) : null;
      const input: SaveDeckInput = {
        archetype: archetypeFromDraft(draft),
        catalogContextId: identity?.catalogContextId ?? null,
        catalogVersion: identity?.catalogVersion ?? null,
        deckIdHash: identity?.deckId ?? null,
        decklistText: analysis.kind === "valid" ? analysis.preview.decklist : "",
        formatLabel: draft.formatLabel.trim() === "" ? null : draft.formatLabel.trim(),
        hashVersion: identity?.hashVersion ?? null,
        id: editing?.id,
        listIdHash: identity?.listId ?? null,
        name: draft.name.trim(),
        rawText: analysis.kind === "valid" ? draft.text : "",
        source: deckSource(draft.text),
      };

      const saved = await decks.save(input);
      if (saved !== null) reset();
    } catch (error) {
      setFormError(String(error));
    }
  };
~~~

### Edit 4.5: Pass errors to the form

**File:** `src/components/DecksTab.tsx`

old_str:
~~~tsx
        <DeckForm
          analysis={analysis}
          draft={draft}
          editing={editing}
          onCancel={reset}
          onChange={(patch) => setDraft((current) => ({ ...current, ...patch }))}
          onSave={() => void save()}
          preview={preview}
          saving={decks.saving}
        />
~~~

new_str:
~~~tsx
        <DeckForm
          analysis={analysis}
          draft={draft}
          editing={editing}
          errors={errors}
          onCancel={reset}
          onChange={(patch) => setDraft((current) => ({ ...current, ...patch }))}
          onSave={() => void save()}
          saving={decks.saving}
          showErrors={showErrors}
        />
~~~

### Edit 4.6: Update the empty-library copy

**File:** `src/components/DecksTab.tsx`

old_str:
~~~tsx
          <p className="mt-1 text-xs text-muted-foreground">Add a PTCGL export, a Limitless paste, or a typed list.</p>
~~~

new_str:
~~~tsx
          <p className="mt-1 text-xs text-muted-foreground">Add a list, or start with a format or archetype.</p>
~~~

### Test 4: typecheck and unit tests

~~~bash
pnpm run typecheck
pnpm test -- --run src/lib/deck-library.test.ts src/lib/deck-draft.test.ts
~~~

Expected: no type errors; both suites pass. The UI has no component test suite;
the form is exercised in Task 8.

### Commit 4

~~~bash
git add src/components/DeckForm.tsx src/components/DecksTab.tsx
git commit -m "feat(decks): allow deck identity without a decklist"
~~~

---

## Task 5: Allow metadata-only decks in the Rust store

**Files:** `src-tauri/src/deck_store.rs`
**Behavior:** accept an empty list with a format; require `metadata` only when there is
no list; reject partial text and invalid archetypes; keep the existing list path.
**Depends on:** Task 4 (shape agreement)

### Edit 5.1: Import `HashSet`

**File:** `src-tauri/src/deck_store.rs`

old_str:
~~~rs
use std::fs;
use std::path::{Path, PathBuf};
~~~

new_str:
~~~rs
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
~~~

### Edit 5.2: Add archetype validation

**File:** `src-tauri/src/deck_store.rs`

old_str:
~~~rs
#[derive(Debug, thiserror::Error)]
pub enum DeckStoreError {
    #[error("deck store: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("deck store folder: {0}")]
    Io(#[from] std::io::Error),
    #[error("deck store: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("deck not found")]
    NotFound,
    #[error("deck library is unavailable")]
    Unavailable,
}
~~~

new_str:
~~~rs
#[derive(Debug, thiserror::Error)]
pub enum DeckStoreError {
    #[error("deck store: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("deck store folder: {0}")]
    Io(#[from] std::io::Error),
    #[error("deck store: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("deck not found")]
    NotFound,
    #[error("deck library is unavailable")]
    Unavailable,
}

fn validate_archetype(archetype: &ArchetypeRef) -> Result<(), DeckStoreError> {
    if archetype.label.trim().is_empty() {
        return Err(DeckStoreError::Invalid("archetype label is required".into()));
    }
    if archetype.cards.len() > 2 {
        return Err(DeckStoreError::Invalid("an archetype has at most two Pokémon".into()));
    }
    let mut seen = HashSet::new();
    for card in &archetype.cards {
        if card.trim().is_empty() {
            return Err(DeckStoreError::Invalid("archetype Pokémon cannot be empty".into()));
        }
        if !seen.insert(card.as_str()) {
            return Err(DeckStoreError::Invalid("archetype Pokémon must be unique".into()));
        }
    }
    Ok(())
}
~~~

### Edit 5.3: Replace the save validation

**File:** `src-tauri/src/deck_store.rs`

old_str:
~~~rs
        self.require_persistent()?;
        let name = input.name.trim();
        if name.is_empty() {
            return Err(DeckStoreError::Invalid("deck name is required".into()));
        }
        if !matches!(input.source.as_str(), "ptcgl" | "limitless" | "typed") {
            return Err(DeckStoreError::Invalid(format!("unsupported deck source: {}", input.source)));
        }
        if input.raw_text.trim().is_empty() || input.decklist_text.trim().is_empty() {
            return Err(DeckStoreError::Invalid("decklist is required".into()));
        }

        let format_label = input.format_label.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        let archetype_json = input.archetype.as_ref().map(serde_json::to_string).transpose()?;
~~~

new_str:
~~~rs
        self.require_persistent()?;
        let name = input.name.trim();
        if name.is_empty() {
            return Err(DeckStoreError::Invalid("deck name is required".into()));
        }
        if !matches!(input.source.as_str(), "ptcgl" | "limitless" | "typed" | "metadata") {
            return Err(DeckStoreError::Invalid(format!("unsupported deck source: {}", input.source)));
        }

        let has_raw_text = !input.raw_text.trim().is_empty();
        let has_decklist_text = !input.decklist_text.trim().is_empty();
        if has_raw_text != has_decklist_text {
            return Err(DeckStoreError::Invalid(
                "raw text and decklist text must both be present or absent".into(),
            ));
        }
        let has_list = has_raw_text && has_decklist_text;
        if input.source == "metadata" && has_list {
            return Err(DeckStoreError::Invalid("metadata source cannot have a decklist".into()));
        }
        if input.source != "metadata" && !has_list {
            return Err(DeckStoreError::Invalid("a decklist is required for this source".into()));
        }

        let format_label = input.format_label.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        if !has_list && format_label.is_none() {
            return Err(DeckStoreError::Invalid("a format is required when there is no decklist".into()));
        }
        if let Some(archetype) = &input.archetype {
            validate_archetype(archetype)?;
        }
        let raw_text = if has_list { input.raw_text } else { String::new() };
        let decklist_text = if has_list { input.decklist_text } else { String::new() };
        let archetype_json = input.archetype.as_ref().map(serde_json::to_string).transpose()?;
~~~

### Edit 5.4: Persist the normalized empty text

**File:** `src-tauri/src/deck_store.rs`

old_str:
~~~rs
                input.source,
                input.raw_text,
                input.decklist_text,
                format_label,
~~~

new_str:
~~~rs
                input.source,
                raw_text,
                decklist_text,
                format_label,
~~~

### Edit 5.5: Add store tests

**File:** `src-tauri/src/deck_store.rs`

Insert after `rejects_an_unknown_source`:

old_str:
~~~rs
    #[test]
    fn in_memory_fallback_reports_the_library_unavailable() {
~~~

new_str:
~~~rs
    #[test]
    fn saves_a_format_only_deck_without_a_list() {
        let mut temp = TempStore::new("metadata");
        let deck = temp
            .store
            .save(SaveDeckInput {
                archetype: Some(ArchetypeRef {
                    cards: vec!["dragapult".into()],
                    id: None,
                    label: "Dragapult".into(),
                }),
                catalog_context_id: None,
                catalog_version: None,
                deck_id_hash: None,
                decklist_text: String::new(),
                format_label: Some("TEF-30C".into()),
                hash_version: None,
                list_id_hash: None,
                raw_text: String::new(),
                source: "metadata".into(),
                ..input(None, "Format only")
            })
            .unwrap();
        assert_eq!(deck.current_version.raw_text, "");
        assert_eq!(deck.current_version.decklist_text, "");
        assert_eq!(deck.current_version.source, "metadata");
        assert_eq!(deck.current_version.deck_id_hash, None);
        assert_eq!(deck.current_version.list_id_hash, None);
        assert_eq!(deck.current_version.hash_version, None);
        assert_eq!(deck.current_version.format_label.as_deref(), Some("TEF-30C"));
        assert_eq!(
            deck.current_version.archetype.as_ref().map(|archetype| archetype.label.as_str()),
            Some("Dragapult")
        );
    }

    #[test]
    fn rejects_metadata_source_with_a_decklist() {
        let mut temp = TempStore::new("metadata-list");
        let error = temp.store.save(SaveDeckInput { source: "metadata".into(), ..input(None, "x") }).unwrap_err();
        assert!(matches!(error, DeckStoreError::Invalid(_)));
    }

    #[test]
    fn rejects_a_no_list_deck_without_a_format() {
        let mut temp = TempStore::new("metadata-no-format");
        let error = temp
            .store
            .save(SaveDeckInput {
                archetype: None,
                catalog_context_id: None,
                catalog_version: None,
                deck_id_hash: None,
                decklist_text: String::new(),
                format_label: None,
                hash_version: None,
                list_id_hash: None,
                raw_text: String::new(),
                source: "metadata".into(),
                ..input(None, "x")
            })
            .unwrap_err();
        assert!(matches!(error, DeckStoreError::Invalid(_)));
    }

    #[test]
    fn rejects_partial_list_text() {
        let mut temp = TempStore::new("partial-list");
        let error = temp.store.save(SaveDeckInput { raw_text: String::new(), ..input(None, "x") }).unwrap_err();
        assert!(matches!(error, DeckStoreError::Invalid(_)));
    }

    #[test]
    fn rejects_duplicate_or_oversized_archetype_cards() {
        let mut temp = TempStore::new("archetype-cards");
        let duplicate = ArchetypeRef {
            cards: vec!["dragapult".into(), "dragapult".into()],
            id: None,
            label: "Dragapult".into(),
        };
        let duplicate_error = temp
            .store
            .save(SaveDeckInput { archetype: Some(duplicate), ..input(None, "x") })
            .unwrap_err();
        assert!(matches!(duplicate_error, DeckStoreError::Invalid(_)));

        let oversized = ArchetypeRef {
            cards: vec!["dragapult".into(), "dusknoir".into(), "blaziken".into()],
            id: None,
            label: "Three".into(),
        };
        let oversized_error = temp
            .store
            .save(SaveDeckInput { archetype: Some(oversized), ..input(None, "x") })
            .unwrap_err();
        assert!(matches!(oversized_error, DeckStoreError::Invalid(_)));
    }

    #[test]
    fn accepts_a_label_only_archetype() {
        let mut temp = TempStore::new("archetype-label");
        let deck = temp
            .store
            .save(SaveDeckInput {
                archetype: Some(ArchetypeRef { cards: vec![], id: None, label: "Custom".into() }),
                ..input(None, "x")
            })
            .unwrap();
        assert_eq!(
            deck.current_version.archetype.as_ref().map(|archetype| archetype.label.as_str()),
            Some("Custom")
        );
    }

    #[test]
    fn a_metadata_only_snapshot_keeps_an_empty_list() {
        let mut temp = TempStore::new("metadata-snapshot");
        let deck = temp
            .store
            .save(SaveDeckInput {
                archetype: Some(ArchetypeRef {
                    cards: vec!["dragapult".into()],
                    id: None,
                    label: "Dragapult".into(),
                }),
                catalog_context_id: None,
                catalog_version: None,
                deck_id_hash: None,
                decklist_text: String::new(),
                format_label: Some("TEF-30C".into()),
                hash_version: None,
                list_id_hash: None,
                raw_text: String::new(),
                source: "metadata".into(),
                ..input(None, "Format only")
            })
            .unwrap();
        let snapshot = temp.store.assign_match_deck("ptcgl-meta.txt", &deck.id).unwrap();
        assert_eq!(snapshot.decklist_text, "");
        assert_eq!(snapshot.deck_id_hash, None);
        assert_eq!(snapshot.list_id_hash, None);
        assert_eq!(snapshot.format_label.as_deref(), Some("TEF-30C"));
    }

    #[test]
    fn in_memory_fallback_reports_the_library_unavailable() {
~~~

### Test 5: Rust tests

~~~bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test deck_store
cd ..
~~~

Expected: all `deck_store` tests pass. `cargo` needs `dist/` from `pnpm build`.

### Commit 5

~~~bash
git add src-tauri/src/deck_store.rs
git commit -m "feat(decks): allow metadata-only deck versions"
~~~

---

## Task 6: Cover metadata-only snapshots and sync payloads

**Files:** `src-tauri/src/sync_worker.rs`
**Behavior:** prove a metadata-only snapshot produces `decklist: ""`, the format,
the archetype, and no identity block.
**Depends on:** Task 5

### Edit 6.1: Add a metadata-only seed helper

**File:** `src-tauri/src/sync_worker.rs`

old_str:
~~~rs
            store.assign_match_deck(LOG, &deck.id).unwrap();
        }
    }

    #[test]
    fn a_tick_reports_whether_it_recorded_an_outcome() {
~~~

new_str:
~~~rs
            store.assign_match_deck(LOG, &deck.id).unwrap();
        }

        fn seed_metadata_deck(&self, format: &str) {
            let mut store = crate::deck_store::lock(&self.worker.decks);
            let deck = store
                .save(SaveDeckInput {
                    id: None,
                    name: "Format only".into(),
                    source: "metadata".into(),
                    raw_text: String::new(),
                    decklist_text: String::new(),
                    format_label: Some(format.into()),
                    archetype: Some(ArchetypeRef {
                        id: None,
                        label: "Custom".into(),
                        cards: vec!["dragapult".into()],
                    }),
                    deck_id_hash: None,
                    list_id_hash: None,
                    hash_version: None,
                    catalog_version: None,
                    catalog_context_id: None,
                })
                .unwrap();
            store.assign_match_deck(LOG, &deck.id).unwrap();
        }
    }

    #[test]
    fn a_tick_reports_whether_it_recorded_an_outcome() {
~~~

### Edit 6.2: Add the payload test

**File:** `src-tauri/src/sync_worker.rs`

old_str:
~~~rs
    #[test]
    fn a_deck_without_a_confirmed_format_is_not_sent() {
~~~

new_str:
~~~rs
    #[test]
    fn a_metadata_only_deck_sends_an_empty_decklist() {
        let rig = Rig::new("deck-metadata");
        rig.seed_metadata_deck("TEF-30C");
        rig.enable_deck_identity();
        rig.enqueue(Release::Now);
        rig.worker.tick(0);
        let body = rig.transport.sent.lock().unwrap()[0].2.clone();
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["deck"]["decklist"], "");
        assert_eq!(json["deck"]["format"], "TEF-30C");
        assert_eq!(json["deck"]["archetype"]["label"], "Custom");
        assert_eq!(json["deck"]["archetype"]["id"], serde_json::Value::Null);
        assert_eq!(json["deck"]["archetype"]["cards"], serde_json::json!(["dragapult"]));
        assert!(json["deck"].get("identity").is_none());
    }

    #[test]
    fn a_deck_without_a_confirmed_format_is_not_sent() {
~~~

### Test 6: Rust tests

~~~bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test sync_worker
cd ..
~~~

Expected: the new payload test and all existing sync tests pass.

### Commit 6

~~~bash
git add src-tauri/src/sync_worker.rs
git commit -m "test(sync): cover metadata-only deck payloads"
~~~

---

## Task 7: Handle empty snapshots in read paths

**Files:** `src/components/DeckSnapshotWarnings.tsx`, `src/components/MatchDeckPanel.tsx`
**Behavior:** empty saved decklists show “No decklist” and never reach the parser,
and the lazy warning chunk is not loaded for them.
**Depends on:** none

### Edit 7.1: Skip the parser for an empty snapshot

**File:** `src/components/DeckSnapshotWarnings.tsx`

old_str:
~~~tsx
  const analysis = useMemo(() => analyzeDecklist(decklistText), [decklistText]);
  const editAction =
    onEditDeck === undefined ? undefined : (
      <Button onClick={onEditDeck} size="sm" variant="outline">
        Edit deck
      </Button>
    );

  if (!analysis.ok) {
~~~

new_str:
~~~tsx
  const hasDecklist = decklistText.trim() !== "";
  const analysis = useMemo(() => (hasDecklist ? analyzeDecklist(decklistText) : null), [decklistText, hasDecklist]);
  const editAction =
    onEditDeck === undefined ? undefined : (
      <Button onClick={onEditDeck} size="sm" variant="outline">
        Edit deck
      </Button>
    );

  if (analysis === null) return null;
  if (!analysis.ok) {
~~~

### Edit 7.2: Add the snapshot empty state

**File:** `src/components/MatchDeckPanel.tsx`

old_str:
~~~tsx
  const [error, setError] = useState<null | string>(null);
  const snapshot = match.deck;
~~~

new_str:
~~~tsx
  const [error, setError] = useState<null | string>(null);
  const snapshot = match.deck;
  const hasDecklist = snapshot !== null && snapshot.decklistText.trim() !== "";
~~~

### Edit 7.3: Replace the preview disclosure

**File:** `src/components/MatchDeckPanel.tsx`

old_str:
~~~tsx
      {snapshot !== null ? (
        <details className="mt-2 text-xs text-muted-foreground">
          <summary className="cursor-pointer">Preview snapshot</summary>
          <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border p-2 font-mono">
            {snapshot.decklistText}
          </pre>
        </details>
      ) : null}

      {snapshot !== null ? (
        <Suspense fallback={null}>
          <DeckSnapshotWarnings
            catalogContextId={snapshot.catalogContextId}
            decklistText={snapshot.decklistText}
            onEditDeck={onOpenDecks}
          />
        </Suspense>
      ) : null}
~~~

new_str:
~~~tsx
      {snapshot !== null ? (
        hasDecklist ? (
          <details className="mt-2 text-xs text-muted-foreground">
            <summary className="cursor-pointer">Preview snapshot</summary>
            <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border p-2 font-mono">
              {snapshot.decklistText}
            </pre>
          </details>
        ) : (
          <p className="mt-2 text-xs text-muted-foreground">No decklist saved for this deck.</p>
        )
      ) : null}

      {snapshot !== null && hasDecklist ? (
        <Suspense fallback={null}>
          <DeckSnapshotWarnings
            catalogContextId={snapshot.catalogContextId}
            decklistText={snapshot.decklistText}
            onEditDeck={onOpenDecks}
          />
        </Suspense>
      ) : null}
~~~

### Test 7: typecheck and unit tests

~~~bash
pnpm run typecheck
pnpm test -- --run src/lib/deck-library.test.ts src/lib/deck-draft.test.ts
~~~

Expected: no type errors; existing tests pass.

### Commit 7

~~~bash
git add src/components/DeckSnapshotWarnings.tsx src/components/MatchDeckPanel.tsx
git commit -m "fix(decks): render metadata-only snapshots without parser errors"
~~~

---

## Task 8: Align the spec and run full verification

**Files:** `docs/specs/2026-10-26-optional-decklist.md`, `.agents/plans/feat-optional-decklist.md`
**Behavior:** record that the draft stores raw species names and resolves them to
slugs on save; run the full gates and manual flows.
**Depends on:** Tasks 1–7

### Edit 8.1: Clarify the draft field

**File:** `docs/specs/2026-10-26-optional-decklist.md`

old_str:
~~~md
  archetypeCards: string[];   // 0–2 species slugs
~~~

new_str:
~~~md
  archetypeSpecies: string[]; // 0–2 species names; resolved to slugs on save
~~~

### Edit 8.2: Match the behavior wording

**File:** `docs/specs/2026-10-26-optional-decklist.md`

old_str:
~~~md
1. **Select catalog entry** → set `archetypeId`, `archetypeLabel`,
   `archetypeCards`, `labelEdited = false`; set `formatLabel` to the catalog
   entry’s `window` (overwrite).
2. **Edit a species slot** → set `archetypeId = null`; update `cards`; if
   `!labelEdited`, set the label to the selected species labels joined with
   `" / "`.
~~~

new_str:
~~~md
1. **Select catalog entry** → set `archetypeId`, `archetypeLabel`,
   `archetypeSpecies`, `labelEdited = false`; set `formatLabel` to the catalog
   entry’s `window` (overwrite).
2. **Edit a species slot** → set `archetypeId = null`; update
   `archetypeSpecies`; if `!labelEdited`, set the label to the selected species
   labels joined with `" / "`.
~~~

### Test 8: Full gates

~~~bash
pnpm install --frozen-lockfile
pnpm run verify
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cd ..
~~~

Manual QA in `pnpm tauri dev`:

1. Create a format-only deck (no list); save; set active; save a match; match
   detail shows the format and “No decklist saved for this deck.”
2. Create a catalog-archetype deck with no list; confirm the format auto-fills;
   edit a species slot; confirm the label derives; edit the label; edit another
   slot; confirm the manual label stays.
3. Create a labelless custom archetype with one species; confirm the save is
   blocked until a label or format is supplied.
4. Create a full-list deck; confirm the preview, format, and save still work.
5. With deck identity on, inspect the sync stub body: metadata-only deck sends
   `decklist: ""`, format, and archetype; no format sends no `deck`.

### Commit 8

~~~bash
git add .agents/plans/feat-optional-decklist.md docs/specs/2026-10-26-optional-decklist.md
git commit -m "docs(decks): align draft species naming"
~~~

---

## Task 9: Review and prepare the PR

**Files:** branch
**Behavior:** run the repo’s `code-review` skill against `refactor/unify-deck-ui`,
resolve findings, and prepare the PR.
**Depends on:** Task 8

### Steps

1. Load the `code-review` skill.
2. Review `git diff refactor/unify-deck-ui...HEAD`.
3. Resolve every Critical/Important finding; commit with conventional messages.
4. Re-run `pnpm run verify` and the Rust gates if code changed.
5. Open the PR with the spec link, the stack note, and the verification output.

### Commit 9

Only if review fixes were needed:

~~~bash
git add <changed files>
git commit -m "fix(decks): address review findings"
~~~




