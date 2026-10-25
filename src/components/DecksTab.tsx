import { useMemo, useState } from "react";

import { LoadingButton } from "@/components/LoadingButton";
import { StatusAlert } from "@/components/StatusAlert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

import type { Deck, SaveDeckInput } from "../lib/tauri";

import { analyzeDecklist, type DeckPreview, identityOf, isFormatWindow } from "../lib/deck-library";
import { useDecks } from "../lib/use-decks";

const BLANK_TEXT = "";
const SOURCE_OPTIONS = ["ptcgl", "limitless", "typed"] as const;

/** The main-window deck library. The capture drawer only needs an active-deck control later. */
export function DecksTab() {
  const decks = useDecks();
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<Deck | null>(null);
  const [name, setName] = useState("");
  const [text, setText] = useState(BLANK_TEXT);
  const [source, setSource] = useState<(typeof SOURCE_OPTIONS)[number]>("typed");
  const [formatLabel, setFormatLabel] = useState("");
  const [archetypeKey, setArchetypeKey] = useState("");
  const [formError, setFormError] = useState<null | string>(null);
  const [pendingArchive, setPendingArchive] = useState<Deck | null>(null);

  const analysis = useMemo(() => analyzeDecklist(text), [text]);
  const preview = analysis.ok ? analysis.preview : null;

  const reset = () => {
    setEditing(null);
    setName("");
    setText(BLANK_TEXT);
    setSource("typed");
    setFormatLabel("");
    setArchetypeKey("");
    setFormError(null);
    setShowForm(false);
  };

  const startAdd = () => {
    reset();
    setShowForm(true);
  };

  const startEdit = (deck: Deck) => {
    const version = deck.currentVersion;
    setEditing(deck);
    setName(deck.name);
    setText(version.rawText);
    setSource(SOURCE_OPTIONS.includes(version.source as (typeof SOURCE_OPTIONS)[number]) ? (version.source as (typeof SOURCE_OPTIONS)[number]) : "typed");
    setFormatLabel(version.formatLabel ?? "");
    setArchetypeKey(version.archetype?.id ?? version.archetype?.label ?? "");
    setFormError(null);
    setShowForm(true);
  };

  const save = async () => {
    if (name.trim() === "") {
      setFormError("Give the deck a name.");
      return;
    }
    if (!analysis.ok) {
      setFormError(analysis.errors.join(" "));
      return;
    }

    try {
      const selected = selectedArchetype(analysis.preview, archetypeKey, editing);
      const identity = await identityOf(analysis.preview.deck);
      const input: SaveDeckInput = {
        archetype: selected,
        catalogContextId: identity.catalogContextId,
        catalogVersion: identity.catalogVersion,
        deckIdHash: identity.deckId,
        decklistText: analysis.preview.decklist,
        formatLabel: formatLabel.trim() === "" ? null : formatLabel.trim(),
        hashVersion: identity.hashVersion,
        id: editing?.id,
        listIdHash: identity.listId,
        name: name.trim(),
        rawText: text,
        source,
      };

      const saved = await decks.save(input);
      if (saved !== null) reset();
    } catch (error) {
      setFormError(String(error));
    }
  };

  const archive = async () => {
    if (pendingArchive === null) return;
    const ok = await decks.archive(pendingArchive.id);
    if (ok) setPendingArchive(null);
  };

  return (
    <div className="space-y-4 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-sm font-semibold">Decks</h2>
          <p className="text-xs text-muted-foreground">Keep a list ready, then choose it when you save a match.</p>
        </div>
        <Button onClick={startAdd} size="sm">Add deck</Button>
      </div>

      {decks.error !== null ? <StatusAlert tone="critical">{decks.error}</StatusAlert> : null}
      {formError !== null ? <StatusAlert tone="critical">{formError}</StatusAlert> : null}

      {showForm ? (
        <DeckForm
          analysis={analysis}
          archetypeKey={archetypeKey}
          editing={editing}
          formatLabel={formatLabel}
          name={name}
          onArchetypeChange={setArchetypeKey}
          onCancel={reset}
          onFormatChange={setFormatLabel}
          onNameChange={setName}
          onSave={() => void save()}
          onSourceChange={setSource}
          onTextChange={setText}
          preview={preview}
          saving={decks.saving}
          source={source}
          text={text}
        />
      ) : null}

      {decks.decks === null ? (
        <p className="text-sm text-muted-foreground">Loading decks…</p>
      ) : decks.decks.length === 0 ? (
        <div className="rounded-md border border-border p-6 text-center">
          <p className="text-sm font-medium">No decks yet</p>
          <p className="mt-1 text-xs text-muted-foreground">Add a PTCGL export, a Limitless paste, or a typed list.</p>
        </div>
      ) : (
        <ul className="space-y-2">
          {decks.decks.map((deck) => (
            <li className="rounded-md border border-border p-3" key={deck.id}>
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="truncate text-sm font-medium">{deck.name}</span>
                    {deck.currentVersion.formatLabel !== null ? (
                      <Badge variant="secondary">{deck.currentVersion.formatLabel}</Badge>
                    ) : null}
                  </div>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    {deck.currentVersion.archetype?.label ?? "No archetype confirmed"}
                  </p>
                </div>
                <div className="flex shrink-0 gap-1">
                  <Button onClick={() => startEdit(deck)} size="sm" variant="outline">Edit</Button>
                  <Button onClick={() => setPendingArchive(deck)} size="sm" variant="ghost">Archive</Button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}

      <AlertDialog onOpenChange={(open) => { if (!open) setPendingArchive(null); }} open={pendingArchive !== null}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Archive this deck?</AlertDialogTitle>
            <AlertDialogDescription>
              It leaves the picker. Saved matches keep their deck snapshot.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => void archive()} variant="destructive">Archive</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function DeckForm({
  analysis,
  archetypeKey,
  editing,
  formatLabel,
  name,
  onArchetypeChange,
  onCancel,
  onFormatChange,
  onNameChange,
  onSave,
  onSourceChange,
  onTextChange,
  preview,
  saving,
  source,
  text,
}: {
  analysis: ReturnType<typeof analyzeDecklist>;
  archetypeKey: string;
  editing: Deck | null;
  formatLabel: string;
  name: string;
  onArchetypeChange: (value: string) => void;
  onCancel: () => void;
  onFormatChange: (value: string) => void;
  onNameChange: (value: string) => void;
  onSave: () => void;
  onSourceChange: (value: (typeof SOURCE_OPTIONS)[number]) => void;
  onTextChange: (value: string) => void;
  preview: DeckPreview | null;
  saving: boolean;
  source: (typeof SOURCE_OPTIONS)[number];
  text: string;
}) {
  const suggestions = preview?.archetypeSuggestions ?? [];
  const storedArchetype = editing?.currentVersion.archetype ?? null;
  const storedOption = storedArchetype !== null && !suggestions.some((suggestion) => suggestion.id === storedArchetype.id)
    ? storedArchetype
    : null;

  return (
    <div className="space-y-3 rounded-md border border-border p-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">{editing === null ? "Add a deck" : `Edit ${editing.name}`}</h3>
        <Button onClick={onCancel} size="sm" variant="ghost">Cancel</Button>
      </div>

      <Field>
        <FieldLabel htmlFor="deck-name">Name</FieldLabel>
        <Input id="deck-name" onChange={(event) => onNameChange(event.target.value)} placeholder="League deck" value={name} />
      </Field>

      <Field>
        <FieldLabel htmlFor="deck-source">Source</FieldLabel>
        <select
          className="h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
          id="deck-source"
          onChange={(event) => onSourceChange(event.target.value as (typeof SOURCE_OPTIONS)[number])}
          value={source}
        >
          {SOURCE_OPTIONS.map((option) => <option key={option} value={option}>{option}</option>)}
        </select>
      </Field>

      <Field data-invalid={!analysis.ok || undefined}>
        <FieldLabel htmlFor="decklist">Decklist</FieldLabel>
        <Textarea
          className="min-h-40 font-mono text-xs"
          id="decklist"
          onChange={(event) => onTextChange(event.target.value)}
          placeholder="Paste a PTCGL export, a Limitless list, or type the list"
          value={text}
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
              onChange={(event) => onFormatChange(event.target.value)}
              placeholder="TEF-30C"
              value={formatLabel}
            />
            <datalist id="deck-format-candidates">
              {preview.formatCandidates.map((candidate) => <option key={candidate} value={candidate} />)}
            </datalist>
            <FieldDescription>
              {formatLabel.trim() === ""
                ? "Pick a fitting window or type a confirmed label."
                : isFormatWindow(formatLabel)
                  ? "A set window."
                  : "Not a set window. It will be stored as a confirmed label."}
            </FieldDescription>
          </Field>

          <Field>
            <FieldLabel htmlFor="deck-archetype">Archetype</FieldLabel>
            <select
              className="h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
              id="deck-archetype"
              onChange={(event) => onArchetypeChange(event.target.value)}
              value={archetypeKey}
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
              {suggestions.length > 0 && suggestions[0] !== undefined
                ? `${evidenceText(suggestions[0])} ${suggestions[0].reasons.join(" ")}`
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

function selectedArchetype(preview: DeckPreview, key: string, editing: Deck | null): SaveDeckInput["archetype"] {
  const suggestion = preview.archetypeSuggestions.find((candidate) => candidate.id === key);
  if (suggestion !== undefined) {
    return { cards: [...suggestion.cards], id: suggestion.id, label: suggestion.label };
  }
  if (key !== "" && editing?.currentVersion.archetype !== null && editing?.currentVersion.archetype !== undefined && (editing.currentVersion.archetype.id ?? editing.currentVersion.archetype.label) === key) {
    return editing.currentVersion.archetype;
  }
  return null;
}
