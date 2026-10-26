import { lazy, Suspense, useState } from "react";

import { LoadingButton } from "@/components/LoadingButton";
import { StatusAlert } from "@/components/StatusAlert";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import type { DecksView } from "../lib/use-decks";
import type { Match } from "../lib/use-matches";

import { hasDecklistText } from "../lib/decklist-text";
import { SELECT_CLASS } from "../lib/form-styles";
import { assignMatchDeck, clearMatchDeck } from "../lib/tauri";
import { DeckOptions } from "./DeckOptions";

const DeckSnapshotWarnings = lazy(() =>
  import("./DeckSnapshotWarnings").then((module) => ({ default: module.DeckSnapshotWarnings })),
);

interface MatchDeckPanelProps {
  decks: DecksView;
  match: Match;
  onChanged: () => void;
  onOpenDecks?: () => void;
}

/** Shows the immutable snapshot and lets the player replace it with the current version. */
export function MatchDeckPanel({ decks, match, onChanged, onOpenDecks }: MatchDeckPanelProps) {
  const [editing, setEditing] = useState(false);
  const [selected, setSelected] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<null | string>(null);
  const snapshot = match.deck;
  const hasDecklist = snapshot !== null && hasDecklistText(snapshot.decklistText);

  const startEditing = () => {
    const available = decks.decks;
    const current = snapshot?.deckId ?? null;
    const keepCurrent =
      current !== null && (available === null || available.some((deck) => deck.id === current));
    setSelected(current !== null && keepCurrent ? current : "");
    setError(null);
    setEditing(true);
  };

  const save = () => {
    setSaving(true);
    setError(null);
    const action =
      selected === "" ? clearMatchDeck(match.log.fileName) : assignMatchDeck(match.log.fileName, selected);
    action
      .then(() => {
        setEditing(false);
        onChanged();
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setSaving(false));
  };

  return (
    <section aria-label="Deck" className="rounded-md border border-border p-3">
      <div className="flex items-start justify-between gap-2">
        <h3 className="text-sm font-semibold">Deck</h3>
        {editing ? null : (
          <Button disabled={decks.decks === null} onClick={startEditing} size="sm" variant="outline">
            Change deck
          </Button>
        )}
      </div>

      {snapshot === null ? (
        <p className="mt-1 text-sm text-muted-foreground">Deck not set</p>
      ) : (
        <dl className="mt-2 grid grid-cols-2 gap-x-3 gap-y-1 text-sm">
          <dt className="text-muted-foreground">Deck</dt>
          <dd className="text-foreground">{snapshot.deckName}</dd>
          <dt className="text-muted-foreground">Format</dt>
          <dd className="text-foreground">{snapshot.formatLabel ?? "Not set"}</dd>
          <dt className="text-muted-foreground">Your archetype</dt>
          <dd className="text-foreground">{snapshot.archetype?.label ?? "Not set"}</dd>
          <dt className="text-muted-foreground">Opponent archetype</dt>
          <dd className="text-foreground">{snapshot.opponentArchetype?.label ?? "Not set"}</dd>
        </dl>
      )}

      {editing ? (
        <div className="mt-3 space-y-2">
          <select
            aria-label="Deck for this match"
            className={cn(SELECT_CLASS, "h-9 w-full")}
            disabled={decks.decks === null || saving}
            onChange={(event) => setSelected(event.target.value)}
            value={selected}
          >
            <option value="">No deck</option>
            <DeckOptions decks={decks.decks} />
          </select>
          <p className="text-xs text-muted-foreground">
            Saving snapshots the deck's current version. The old match snapshot is replaced; the
            deck version itself is unchanged.
          </p>
          {error !== null ? <StatusAlert tone="critical">{error}</StatusAlert> : null}
          <div className="flex gap-2">
            <LoadingButton disabled={decks.decks === null} loading={saving} onClick={save} size="sm">
              Save
            </LoadingButton>
            <Button disabled={saving} onClick={() => setEditing(false)} size="sm" variant="ghost">
              Cancel
            </Button>
          </div>
        </div>
      ) : null}

      {snapshot !== null ? (
        hasDecklist ? (
          <details className="mt-2 text-xs text-muted-foreground">
            <summary className="cursor-pointer">Preview snapshot</summary>
            <pre className="mt-2 max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border p-2 font-mono">
              {snapshot.decklistText}
            </pre>
          </details>
        ) : (
          <p className="mt-2 text-xs text-muted-foreground">No decklist saved for this deck.</p>
        )
      ) : null}

      {snapshot !== null && hasDecklist ? (
        <Suspense fallback={null}>
          <DeckSnapshotWarnings
            catalogContextId={snapshot.catalogContextId}
            decklistText={snapshot.decklistText}
            onEditDeck={onOpenDecks}
          />
        </Suspense>
      ) : null}

      {decks.error !== null ? <p className="mt-2 text-xs text-destructive">{decks.error}</p> : null}
    </section>
  );
}
