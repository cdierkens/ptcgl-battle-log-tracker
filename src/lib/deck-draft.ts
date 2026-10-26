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
