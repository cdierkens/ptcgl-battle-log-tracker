import type { ArchetypeSuggestion } from "@dierkens.dev/ptcg-decklist-tools";

import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

import type { DeckDraft, DeckDraftErrors } from "../lib/deck-draft";

import {
  applyArchetypeSelection,
  clearArchetype,
  deriveArchetypeLabelIfNeeded,
  setArchetypeLabel,
  setArchetypeSlot,
} from "../lib/deck-draft";
import { ARCHETYPES, findArchetype, FORMAT_WINDOWS, SPECIES } from "../lib/deck-library";
import { SELECT_CLASS } from "../lib/form-styles";

const CUSTOM_ARCHETYPE = "__custom__";
const ARCHETYPE_ERROR_ID = "deck-archetype-error";

interface ArchetypeFieldsProps {
  draft: DeckDraft;
  errors: DeckDraftErrors;
  onChange: (next: DeckDraft) => void;
  showErrors: boolean;
  suggestions: readonly ArchetypeSuggestion[];
}

export function ArchetypeFields({ draft, errors, onChange, showErrors, suggestions }: ArchetypeFieldsProps) {
  const shownSuggestion = suggestions.find((suggestion) => suggestion.id === draft.archetypeId);
  const hasArchetype =
    draft.archetypeId !== null ||
    draft.archetypeLabel.trim() !== "" ||
    draft.archetypeSpecies.some((value) => value.trim() !== "");
  const selectedCatalogId =
    draft.archetypeId !== null && findArchetype(draft.archetypeId) !== undefined
      ? draft.archetypeId
      : hasArchetype
        ? CUSTOM_ARCHETYPE
        : "";
  const suggestionIds = new Set(suggestions.map((suggestion) => suggestion.id));
  const catalogOptions = ARCHETYPES.filter((archetype) => !suggestionIds.has(archetype.id));
  const catalogGroups = FORMAT_WINDOWS.map((window) => ({
    options: catalogOptions.filter((archetype) => archetype.window === window),
    window,
  })).filter((group) => group.options.length > 0);
  const archetypeInvalid = showErrors && errors.archetype !== null;
  const describedBy = archetypeInvalid ? ARCHETYPE_ERROR_ID : undefined;

  const selectArchetype = (value: string) => {
    if (value === "") {
      onChange(clearArchetype(draft));
      return;
    }
    if (value === CUSTOM_ARCHETYPE) return;
    onChange(applyArchetypeSelection(draft, value, suggestions));
  };

  return (
    <>
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
          {catalogGroups.map((group) => (
            <optgroup key={group.window} label={group.window}>
              {group.options.map((archetype) => (
                <option key={archetype.id} value={archetype.id}>
                  {archetype.label}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
        <FieldDescription>
          Select a catalog archetype, or enter up to two Pokémon below.
          {shownSuggestion === undefined ? "" : ` ${evidenceText(shownSuggestion)} ${shownSuggestion.reasons.join(" ")}`}
        </FieldDescription>
      </Field>

      <Field>
        <FieldLabel htmlFor="deck-archetype-label">Archetype label</FieldLabel>
        <Input
          aria-describedby={describedBy}
          aria-invalid={archetypeInvalid || undefined}
          id="deck-archetype-label"
          onBlur={() => onChange(deriveArchetypeLabelIfNeeded(draft))}
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
              aria-describedby={describedBy}
              aria-invalid={archetypeInvalid || undefined}
              id={`deck-species-${index}`}
              list="deck-species-candidates"
              onBlur={() => onChange(deriveArchetypeLabelIfNeeded(draft))}
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
      {archetypeInvalid ? <FieldError id={ARCHETYPE_ERROR_ID}>{errors.archetype}</FieldError> : null}
    </>
  );
}

function evidenceText(suggestion: ArchetypeSuggestion): string {
  if (suggestion.evidence.kind === "alias") {
    const matched = suggestion.evidence.matched.map((match) => `${match.count}× ${match.name}`).join(", ");
    return `Matched ${matched} in ${suggestion.evidence.windows.join(", ")}.`;
  }
  return `Line ${suggestion.evidence.line}: ${suggestion.evidence.copies} cards across ${suggestion.evidence.stages} stage(s), ${suggestion.evidence.traits.join(", ")}.`;
}
