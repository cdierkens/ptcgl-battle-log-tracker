import { ArchetypeFields } from "@/components/ArchetypeFields";
import { LoadingButton } from "@/components/LoadingButton";
import { StatusAlert } from "@/components/StatusAlert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

import type { DeckDraft, DeckDraftErrors } from "../lib/deck-draft";
import type { DeckDraftAnalysis } from "../lib/deck-library";
import type { Deck } from "../lib/tauri";

import { FORMAT_WINDOWS, isFormatWindow } from "../lib/deck-library";

interface DeckFormProps {
  analysis: DeckDraftAnalysis;
  draft: DeckDraft;
  editing: Deck | null;
  errors: DeckDraftErrors;
  onCancel: () => void;
  onChange: (next: DeckDraft) => void;
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
  const formatCandidates = preview?.formatCandidates ?? FORMAT_WINDOWS;
  const formatValue = draft.formatLabel.trim();
  const formatDescription =
    formatValue === ""
      ? "Pick a fitting window or type a confirmed label."
      : isFormatWindow(formatValue)
        ? "A set window."
        : "Not a set window. It will be stored as a confirmed label.";

  return (
    <div className="space-y-3 rounded-md border border-border p-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">{editing === null ? "Add a deck" : `Edit ${editing.name}`}</h3>
        <Button onClick={onCancel} size="sm" variant="ghost">Cancel</Button>
      </div>

      <Field data-invalid={(showErrors && errors.name !== null) || undefined}>
        <FieldLabel htmlFor="deck-name">Name</FieldLabel>
        <Input
          id="deck-name"
          onChange={(event) => onChange({ ...draft, name: event.target.value })}
          placeholder="League deck"
          value={draft.name}
        />
        <FieldError>{showErrors ? errors.name : null}</FieldError>
      </Field>

      <Field data-invalid={(showErrors && errors.decklist !== null) || undefined}>
        <FieldLabel htmlFor="decklist">Decklist</FieldLabel>
        <Textarea
          className="min-h-40 font-mono text-xs"
          id="decklist"
          onChange={(event) => onChange({ ...draft, text: event.target.value })}
          placeholder="Optional: paste a PTCGL export, a Limitless list, or type the list"
          value={draft.text}
        />
        <FieldDescription>
          Optional. Parsed locally. The original text and a portable PTCGL render are stored when present.
        </FieldDescription>
        <FieldError>{showErrors ? errors.decklist : null}</FieldError>
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
          onChange={(event) => onChange({ ...draft, formatLabel: event.target.value })}
          placeholder="TEF-30C"
          value={draft.formatLabel}
        />
        <datalist id="deck-format-candidates">
          {formatCandidates.map((candidate) => <option key={candidate} value={candidate} />)}
        </datalist>
        <FieldDescription>{formatDescription}</FieldDescription>
        <FieldError>{showErrors ? errors.format : null}</FieldError>
      </Field>

      <ArchetypeFields
        draft={draft}
        errors={errors}
        onChange={onChange}
        showErrors={showErrors}
        suggestions={suggestions}
      />

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
