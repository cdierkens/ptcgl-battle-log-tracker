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
