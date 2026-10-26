import { useMemo } from "react";

import { Button } from "@/components/ui/button";

import { analyzeDecklistSnapshot } from "../lib/deck-library";
import { StatusAlert } from "./StatusAlert";

/**
 * Re-checks a saved snapshot with the current catalog. This is a lazy chunk so
 * the decklist package does not load until someone opens a match's deck.
 */
export function DeckSnapshotWarnings({
  catalogContextId,
  decklistText,
  onEditDeck,
}: {
  catalogContextId: null | string;
  decklistText: string;
  onEditDeck?: () => void;
}) {
  const analysis = useMemo(() => analyzeDecklistSnapshot(decklistText), [decklistText]);
  const editAction =
    onEditDeck === undefined ? undefined : (
      <Button onClick={onEditDeck} size="sm" variant="outline">
        Edit deck
      </Button>
    );

  if (analysis === null) return null;
  if (!analysis.ok) {
    return (
      <StatusAlert actions={editAction} tone="info">
        Couldn't re-check this deck snapshot with the current catalog: {analysis.errors.join(" ")}
      </StatusAlert>
    );
  }

  const { deck, unresolved, warnings } = analysis.preview;
  const currentContext = deck.catalogContextId ?? null;
  const contextChanged =
    catalogContextId !== null && currentContext !== null && currentContext !== catalogContextId;
  const notes = [
    ...(unresolved > 0 ? [`${unresolved} card${unresolved === 1 ? "" : "s"} not in the current catalog.`] : []),
    ...warnings,
    ...(contextChanged ? ["This deck was saved with a different catalog context; re-import it to refresh its identity."] : []),
  ];

  if (notes.length === 0) return null;

  return (
    <StatusAlert actions={editAction} tone="info">
      {notes.join(" ")}
    </StatusAlert>
  );
}
