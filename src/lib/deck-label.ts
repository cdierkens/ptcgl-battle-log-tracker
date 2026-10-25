import type { Deck } from "./tauri";

/** The compact picker label: name plus the confirmed format when present. */
export function deckLabel(deck: Deck): string {
  return deck.currentVersion.formatLabel === null ? deck.name : `${deck.name} · ${deck.currentVersion.formatLabel}`;
}
