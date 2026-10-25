import { useMemo, useState } from "react";

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

import type { Deck, SaveDeckInput } from "../lib/tauri";

import { analyzeDecklist, identityOf } from "../lib/deck-library";
import { useDecks } from "../lib/use-decks";
import { ActiveDeckPicker } from "./ActiveDeckPicker";
import { DeckForm, type DeckSource, selectedArchetype, SOURCE_OPTIONS } from "./DeckForm";

const BLANK_TEXT = "";

/** The main-window deck library. The capture drawer gets the compact active-deck control. */
export function DecksTab() {
  const decks = useDecks();
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<Deck | null>(null);
  const [name, setName] = useState("");
  const [text, setText] = useState(BLANK_TEXT);
  const [source, setSource] = useState<DeckSource>("typed");
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
    setSource(SOURCE_OPTIONS.includes(version.source as DeckSource) ? (version.source as DeckSource) : "typed");
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
          <p className="text-xs text-muted-foreground">Keep a list ready for the matches you save.</p>
        </div>
        <Button onClick={startAdd} size="sm">Add deck</Button>
      </div>

      {decks.error !== null ? <StatusAlert tone="critical">{decks.error}</StatusAlert> : null}
      {formError !== null ? <StatusAlert tone="critical">{formError}</StatusAlert> : null}

      <ActiveDeckPicker
        activeDeck={decks.activeDeck}
        decks={decks.decks}
        disabled={decks.saving}
        error={decks.error}
        onChange={(id) => void decks.setActive(id)}
      />

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
        decks.error === null ? <p className="text-sm text-muted-foreground">Loading decks…</p> : null
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
                    {decks.activeDeck?.id === deck.id ? <Badge>Active</Badge> : null}
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
