import type { ArchetypeSuggestion, CatalogArchetype } from "@dierkens.dev/ptcg-decklist-tools";

import type { DeckDraftAnalysis } from "./deck-library";
import type { ArchetypeRef, Deck } from "./tauri";

import {
  deriveArchetypeLabel,
  findArchetype,
  findSpecies,
  resolveArchetypeSlots,
  speciesSlots,
} from "./deck-library";

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

export function applyArchetypeSelection(
  draft: DeckDraft,
  id: string,
  suggestions: readonly ArchetypeSuggestion[],
): DeckDraft {
  const catalog = findArchetype(id);
  if (catalog !== undefined) return applyCatalogArchetype(draft, catalog);
  const suggestion = suggestions.find((candidate) => candidate.id === id);
  return suggestion === undefined ? draft : applySuggestedArchetype(draft, suggestion);
}

export function applyCatalogArchetype(draft: DeckDraft, archetype: CatalogArchetype): DeckDraft {
  return {
    ...draft,
    archetypeId: archetype.id,
    archetypeLabel: archetype.label,
    archetypeSpecies: speciesSlots(archetype.cards),
    formatLabel: archetype.window,
    labelEdited: false,
  };
}

/** Only for synthetic suggestions; catalog ids are resolved first. */
export function applySuggestedArchetype(draft: DeckDraft, suggestion: ArchetypeSuggestion): DeckDraft {
  return {
    ...draft,
    archetypeId: null,
    archetypeLabel: suggestion.label,
    archetypeSpecies: speciesSlots(suggestion.cards),
    labelEdited: false,
  };
}

export function archetypeFromDraft(draft: DeckDraft): ArchetypeRef | null {
  const { cards, unknown } = resolveArchetypeSlots(draft.archetypeSpecies);
  if (unknown) return null;
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
  const { duplicate, unknown } = resolveArchetypeSlots(draft.archetypeSpecies);
  let archetype: null | string = unknown
    ? "Choose a Pokémon from the list."
    : duplicate
      ? "Choose two different Pokémon."
      : null;
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

export function deriveArchetypeLabelIfNeeded(draft: DeckDraft): DeckDraft {
  if (draft.archetypeLabel.trim() !== "") return draft;
  const label = deriveArchetypeLabel(draft.archetypeSpecies);
  if (label === "") return draft;
  return { ...draft, archetypeLabel: label, labelEdited: false };
}

export function draftFromDeck(deck: Deck): DeckDraft {
  const version = deck.currentVersion;
  const archetype = version.archetype;
  const storedId = archetype?.id ?? null;
  const archetypeId = storedId !== null && findArchetype(storedId) !== undefined ? storedId : null;
  return {
    archetypeId,
    archetypeLabel: archetype?.label ?? "",
    archetypeSpecies: speciesSlots(archetype?.cards ?? []),
    formatLabel: version.formatLabel ?? "",
    labelEdited: archetype !== null && archetypeId === null,
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
  const selected = findSpecies(value);
  if (selected !== undefined) {
    const other = index === 0 ? 1 : 0;
    if (findSpecies(slots[other] ?? "")?.slug === selected.slug) slots[other] = "";
  }
  const species = slots.filter((slot) => slot.trim() !== "");
  return {
    ...draft,
    archetypeId: null,
    archetypeLabel: draft.labelEdited ? draft.archetypeLabel : deriveArchetypeLabel(species),
    archetypeSpecies: slots,
  };
}
