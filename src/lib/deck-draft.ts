import type { ArchetypeSuggestion, CatalogArchetype } from "@dierkens.dev/ptcg-decklist-tools";

import type { CatalogSpecies, DeckDraftAnalysis } from "./deck-library";
import type { ArchetypeRef, Deck } from "./tauri";

import { ARCHETYPES, deriveArchetypeLabel, findSpecies, speciesSlots } from "./deck-library";

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
  return {
    ...draft,
    archetypeId: archetype.id,
    archetypeLabel: archetype.label,
    archetypeSpecies: speciesSlots(archetype.cards),
    formatLabel: archetype.window,
    labelEdited: false,
  };
}

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
  const resolved = draft.archetypeSpecies.map((value) => (value.trim() === "" ? null : findSpecies(value)));
  if (resolved.some((species) => species === undefined)) return null;
  const cards = [
    ...new Set(
      resolved
        .filter((species): species is CatalogSpecies => species !== null && species !== undefined)
        .map((species) => species.slug),
    ),
  ];
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
  const archetypeId = storedId !== null && ARCHETYPES.some((entry) => entry.id === storedId) ? storedId : null;
  return {
    archetypeId,
    archetypeLabel: archetype?.label ?? "",
    archetypeSpecies: speciesSlots(archetype?.cards ?? []),
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
