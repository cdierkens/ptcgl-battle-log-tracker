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

import type { DeckSource } from "./tauri";

import { isBlank } from "./decklist-text";

export interface CatalogSpecies {
  label: string;
  slug: string;
}

export type DeckDraftAnalysis =
  | { errors: string[]; kind: "invalid" }
  | { kind: "empty" }
  | { kind: "valid"; preview: DeckPreview };

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

export type DetectedDeckSource = Exclude<DeckSource, "limitless">;

export interface ResolvedArchetypeSlots {
  /** De-duplicated species slugs. */
  cards: string[];
  /** True when the slots repeat a species. */
  duplicate: boolean;
  labels: string[];
  unknown: boolean;
}

const CATALOG = catalog;

const titleCase = (value: string) =>
  value.length === 0 ? value : value.charAt(0).toUpperCase() + value.slice(1);

function titleCaseSlug(slug: string): string {
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
  SPECIES_BY_SLUG.set(card.species, { label: titleCaseSlug(card.species), slug: card.species });
}

export const SPECIES: readonly CatalogSpecies[] = [...SPECIES_BY_SLUG.values()].sort((left, right) =>
  left.label.localeCompare(right.label),
);

const SPECIES_BY_LABEL = new Map(SPECIES.map((species) => [species.label.toLowerCase(), species]));

export const ARCHETYPES: readonly CatalogArchetype[] = archetypes;
export const FORMAT_WINDOWS: readonly string[] = [...new Set(archetypes.map((archetype) => archetype.window))].sort();

/** Wrap the parser for the form: blank is a valid no-list state, not an error. */
export function analyzeDeckDraft(text: string): DeckDraftAnalysis {
  if (isBlank(text)) return { kind: "empty" };
  const result = analyzeDecklist(text);
  return result.ok ? { kind: "valid", preview: result.preview } : { errors: result.errors, kind: "invalid" };
}

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

/** Empty snapshots skip the parser entirely; there is nothing to re-check. */
export function analyzeDecklistSnapshot(text: string): DeckPreviewResult | null {
  return isBlank(text) ? null : analyzeDecklist(text);
}

/**
 * Provenance only; parsing accepts any of these shapes. Section headers plus a
 * Total Cards line identify a PTCGL/Limitless export; anything else is typed.
 * Blank text means the deck has no list, not that it was typed.
 */
export function deckSource(text: string): DetectedDeckSource {
  if (isBlank(text)) return "metadata";
  const sections = /^\s*(Pokémon|Pokemon|Trainer|Energy)\s*:\s*\d+/m.test(text);
  const total = /^\s*Total Cards\s*:\s*\d+/im.test(text);
  return sections && total ? "ptcgl" : "typed";
}

export function deriveArchetypeLabel(values: readonly string[]): string {
  return [...new Set(resolveArchetypeSlots(values).labels)].join(" / ");
}

export function findArchetype(id: string): CatalogArchetype | undefined {
  return ARCHETYPES.find((archetype) => archetype.id === id);
}

export function findSpecies(value: string): CatalogSpecies | undefined {
  const key = value.trim().toLowerCase();
  return SPECIES_BY_SLUG.get(key) ?? SPECIES_BY_LABEL.get(key);
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

export function resolveArchetypeSlots(values: readonly string[]): ResolvedArchetypeSlots {
  const trimmed = values.map((value) => value.trim()).filter((value) => value !== "");
  const found = trimmed.map((value) => findSpecies(value));
  const resolved = found.filter((species): species is CatalogSpecies => species !== undefined);
  const cards = [...new Set(resolved.map((species) => species.slug))];
  return {
    cards,
    duplicate: cards.length !== resolved.length,
    labels: trimmed.map((value, index) => found[index]?.label ?? value),
    unknown: found.some((species) => species === undefined),
  };
}

export function speciesLabelFor(slug: string): string {
  return SPECIES_BY_SLUG.get(slug)?.label ?? titleCaseSlug(slug);
}

/** The two display slots for an archetype's 0–2 species slugs. */
export function speciesSlots(cards: readonly string[]): string[] {
  return [
    cards[0] === undefined ? "" : speciesLabelFor(cards[0]),
    cards[1] === undefined ? "" : speciesLabelFor(cards[1]),
  ];
}

