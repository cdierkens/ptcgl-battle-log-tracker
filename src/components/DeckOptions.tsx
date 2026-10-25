import type { Deck } from "../lib/tauri";

import { deckLabel } from "../lib/deck-label";

/** Shared deck picker options; the surrounding select owns the empty choice. */
export function DeckOptions({ decks }: { decks: Deck[] | null }) {
  return (
    <>
      {(decks ?? []).map((deck) => (
        <option key={deck.id} value={deck.id}>
          {deckLabel(deck)}
        </option>
      ))}
    </>
  );
}
