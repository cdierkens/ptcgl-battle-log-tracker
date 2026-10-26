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
  deriveArchetypeLabelIfNeeded,
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
  const catalogGroups = FORMAT_WINDOWS.map((window) => ({
    options: catalogOptions.filter((archetype) => archetype.window === window),
    window,
  })).filter((group) => group.options.length > 0);
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
