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
