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

import {
  archetypeFromDraft,
  BLANK_DECK_DRAFT,
  type DeckDraft,
  deckDraftErrors,
  draftFromDeck,
} from "../lib/deck-draft";
import { analyzeDeckDraft, deckSource, identityOf } from "../lib/deck-library";
import { useDecks } from "../lib/use-decks";
import { ActiveDeckPicker } from "./ActiveDeckPicker";
import { DeckForm } from "./DeckForm";

/** The main-window deck library. The capture drawer gets the compact active-deck control. */
export function DecksTab() {
  const decks = useDecks();
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<Deck | null>(null);
  const [draft, setDraft] = useState<DeckDraft>(BLANK_DECK_DRAFT);
  const [formError, setFormError] = useState<null | string>(null);
  const [pendingArchive, setPendingArchive] = useState<Deck | null>(null);
  const [showErrors, setShowErrors] = useState(false);

  const analysis = useMemo(() => analyzeDeckDraft(draft.text), [draft.text]);
  const errors = deckDraftErrors(analysis, draft);

  const reset = () => {
    setEditing(null);
    setDraft(BLANK_DECK_DRAFT);
    setFormError(null);
    setShowErrors(false);
    setShowForm(false);
  };

  const startAdd = () => {
    reset();
    setShowForm(true);
  };

  const startEdit = (deck: Deck) => {
    setEditing(deck);
    setDraft(draftFromDeck(deck));
    setFormError(null);
    setShowErrors(false);
    setShowForm(true);
  };

  const save = async () => {
    setShowErrors(true);
    const currentErrors = deckDraftErrors(analysis, draft);
    const firstError =
      currentErrors.name ?? currentErrors.decklist ?? currentErrors.format ?? currentErrors.archetype;
    if (firstError !== null) {
      setFormError(firstError);
      return;
    }

    try {
      const identity = analysis.kind === "valid" ? await identityOf(analysis.preview.deck) : null;
      const input: SaveDeckInput = {
        archetype: archetypeFromDraft(draft),
        catalogContextId: identity?.catalogContextId ?? null,
        catalogVersion: identity?.catalogVersion ?? null,
        deckIdHash: identity?.deckId ?? null,
        decklistText: analysis.kind === "valid" ? analysis.preview.decklist : "",
        formatLabel: draft.formatLabel.trim() === "" ? null : draft.formatLabel.trim(),
        hashVersion: identity?.hashVersion ?? null,
        id: editing?.id,
        listIdHash: identity?.listId ?? null,
        name: draft.name.trim(),
        rawText: analysis.kind === "valid" ? draft.text : "",
        source: deckSource(draft.text),
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
          draft={draft}
          editing={editing}
          errors={errors}
          onCancel={reset}
          onChange={(patch) => setDraft((current) => ({ ...current, ...patch }))}
          onSave={() => void save()}
          saving={decks.saving}
          showErrors={showErrors}
        />
      ) : null}

      {decks.decks === null ? (
        decks.error === null ? <p className="text-sm text-muted-foreground">Loading decks…</p> : null
      ) : decks.decks.length === 0 ? (
        <div className="rounded-md border border-border p-6 text-center">
          <p className="text-sm font-medium">No decks yet</p>
          <p className="mt-1 text-xs text-muted-foreground">Add a list, or start with a format or archetype.</p>
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
