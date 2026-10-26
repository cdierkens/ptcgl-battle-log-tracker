import type { ArchetypeSuggestion } from "@dierkens.dev/ptcg-decklist-tools";

import { describe, expect, it } from "vitest";

import type { DeckDraftAnalysis } from "./deck-library";
import type { Deck } from "./tauri";

import {
  applyArchetypeSelection,
  applyCatalogArchetype,
  archetypeFromDraft,
  BLANK_DECK_DRAFT,
  clearArchetype,
  deckDraftErrors,
  deriveArchetypeLabelIfNeeded,
  draftFromDeck,
  setArchetypeLabel,
  setArchetypeSlot,
} from "./deck-draft";
import { ARCHETYPES, speciesSlots } from "./deck-library";

const DRAGAPULT = ARCHETYPES.find((archetype) => archetype.label === "Dragapult ex");
if (DRAGAPULT === undefined) throw new Error("catalog is missing Dragapult ex");

const catalogSuggestion: ArchetypeSuggestion = {
  basis: "alias",
  cards: DRAGAPULT.cards,
  confidence: 0.9,
  evidence: { competitors: [], kind: "alias", matched: [], specificity: 1, windows: [DRAGAPULT.window] },
  id: DRAGAPULT.id,
  label: DRAGAPULT.label,
  reasons: [],
};

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
    expect(draft.archetypeSpecies).toEqual(speciesSlots(DRAGAPULT.cards));
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

  it("drops a stored id that is not in the catalog", () => {
    const syntheticDeck: Deck = {
      ...deck,
      currentVersion: {
        ...deck.currentVersion,
        archetype: { cards: ["dragapult"], id: "synthetic:1", label: "Synthetic" },
      },
    };
    expect(draftFromDeck(syntheticDeck).archetypeId).toBeNull();
    expect(draftFromDeck(syntheticDeck).labelEdited).toBe(true);
  });
});

describe("archetype draft transitions", () => {
  it("fills label, cards, and format from a catalog archetype", () => {
    const draft = applyCatalogArchetype(BLANK_DECK_DRAFT, DRAGAPULT);
    expect(draft.archetypeId).toBe(DRAGAPULT.id);
    expect(draft.archetypeLabel).toBe(DRAGAPULT.label);
    expect(draft.archetypeSpecies).toEqual(speciesSlots(DRAGAPULT.cards));
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

  it("keeps catalog identity when a suggestion is a catalog entry", () => {
    const selected = applyArchetypeSelection(BLANK_DECK_DRAFT, catalogSuggestion.id, [catalogSuggestion]);
    expect(selected.archetypeId).toBe(DRAGAPULT.id);
    expect(selected.formatLabel).toBe(DRAGAPULT.window);
    expect(selected.archetypeSpecies).toEqual(speciesSlots(DRAGAPULT.cards));
  });

  it("treats a synthetic suggestion as custom and keeps the format", () => {
    const synthetic: ArchetypeSuggestion = {
      ...catalogSuggestion,
      cards: ["dragapult"],
      id: "line:1",
      label: "Synthetic line",
    };
    const selected = applyArchetypeSelection(
      { ...BLANK_DECK_DRAFT, formatLabel: "SVI-ASC" },
      synthetic.id,
      [synthetic],
    );
    expect(selected.archetypeId).toBeNull();
    expect(selected.formatLabel).toBe("SVI-ASC");
    expect(selected.archetypeLabel).toBe("Synthetic line");
    expect(selected.archetypeSpecies).toEqual(["Dragapult", ""]);
  });

  it("ignores an unknown selection", () => {
    expect(applyArchetypeSelection(BLANK_DECK_DRAFT, "missing", [])).toBe(BLANK_DECK_DRAFT);
  });

  it("resets catalog state when a catalog entry is selected again", () => {
    const custom = setArchetypeLabel(applyCatalogArchetype(BLANK_DECK_DRAFT, DRAGAPULT), "My Dragapult");
    const selected = applyCatalogArchetype(custom, DRAGAPULT);
    expect(selected.archetypeId).toBe(DRAGAPULT.id);
    expect(selected.archetypeLabel).toBe(DRAGAPULT.label);
    expect(selected.archetypeSpecies).toEqual(speciesSlots(DRAGAPULT.cards));
    expect(selected.formatLabel).toBe(DRAGAPULT.window);
    expect(selected.labelEdited).toBe(false);
  });

  it("clears the other slot when the same species is chosen", () => {
    const first = setArchetypeSlot(BLANK_DECK_DRAFT, 0, "Dragapult");
    const duplicate = setArchetypeSlot(first, 1, "Dragapult");
    expect(duplicate.archetypeSpecies).toEqual(["", "Dragapult"]);
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

  it("allows a label-only archetype", () => {
    const draft = { ...BLANK_DECK_DRAFT, archetypeLabel: "Custom" };
    expect(archetypeFromDraft(draft)).toEqual({ cards: [], id: null, label: "Custom" });
  });

  it("returns null when a species is not in the catalog", () => {
    const draft = { ...BLANK_DECK_DRAFT, archetypeLabel: "X", archetypeSpecies: ["Not a Pokémon", ""] };
    expect(archetypeFromDraft(draft)).toBeNull();
  });

  it("de-duplicates species before saving", () => {
    const draft = { ...BLANK_DECK_DRAFT, archetypeLabel: "X", archetypeSpecies: ["Dragapult", "Dragapult"] };
    expect(archetypeFromDraft(draft)).toEqual({ cards: ["dragapult"], id: null, label: "X" });
  });

  it("derives a cleared label on blur", () => {
    const catalogDraft = applyCatalogArchetype(BLANK_DECK_DRAFT, DRAGAPULT);
    const cleared = setArchetypeLabel(catalogDraft, "");
    const derived = deriveArchetypeLabelIfNeeded(cleared);
    expect(derived.archetypeLabel).toBe("Dragapult");
    expect(derived.labelEdited).toBe(false);
  });

  it("leaves a non-empty label alone on blur", () => {
    const labeled = setArchetypeLabel(BLANK_DECK_DRAFT, "My Dragapult");
    expect(deriveArchetypeLabelIfNeeded(labeled)).toBe(labeled);
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
