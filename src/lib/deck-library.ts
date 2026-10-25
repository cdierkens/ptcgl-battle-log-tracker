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

export interface DeckIdentity {
  catalogContextId: null | string;
  catalogVersion: string;
  deckId: string;
  hashVersion: number;
  listId: string;
}

export interface DeckPreview {
  archetypeSuggestions: ArchetypeSuggestion[];
  deck: NormalizedDeck;
  decklist: string;
  formatCandidates: string[];
  sections: { count: number; section: string }[];
  unresolved: number;
  warnings: string[];
}

export type DeckPreviewResult =
  | { errors: string[]; ok: false; }
  | { ok: true; preview: DeckPreview };

const CATALOG = catalog;

/** Parse, normalise, render, and read the projections the deck editor needs. Never throws. */
export function analyzeDecklist(text: string): DeckPreviewResult {
  const parsed = parseDecklist(text);
  if (!parsed.ok) return { errors: parsed.error.map((error) => error.message), ok: false };

  const normalized = normalizeDecklist(parsed.value, { catalog: CATALOG });
  if (!normalized.ok) return { errors: normalized.error.map((error) => error.message), ok: false };

  const deck = normalized.value;
  const view = deckView(deck);

  return {
    ok: true,
    preview: {
      archetypeSuggestions: suggestArchetype(deck),
      deck,
      decklist: renderDecklist(deck, "ptcgl"),
      formatCandidates: fittingWindows(deck).map((window) => window.label),
      sections: view.sections.map((section) => ({ count: section.count, section: section.section })),
      unresolved: deck.entries.filter((entry) => entry.card === null).length,
      warnings: deck.warnings.map((warning) => warning.message),
    },
  };
}

/**
 * Provenance only; parsing accepts any of these shapes. Section headers plus a
 * Total Cards line identify a PTCGL/Limitless export; anything else is typed.
 */
export function deckSource(text: string): "ptcgl" | "typed" {
  const sections = /^\s*(Pokémon|Pokemon|Trainer|Energy)\s*:\s*\d+/m.test(text);
  const total = /^\s*Total Cards\s*:\s*\d+/im.test(text);
  return sections && total ? "ptcgl" : "typed";
}

/** The two hashes and the versions needed to compare them later. */
export async function identityOf(deck: NormalizedDeck): Promise<DeckIdentity> {
  const [functional, exact] = await Promise.all([deckId(deck), listId(deck)]);
  return {
    catalogContextId: deck.catalogContextId ?? null,
    catalogVersion: functional.catalogVersion,
    deckId: functional.value,
    hashVersion: functional.hashVersion,
    listId: exact.value,
  };
}

/** True when the label resolves as a set window; false means an opaque confirmed label. */
export function isFormatWindow(label: string): boolean {
  return !isErr(parseFormatLabel(label));
}
