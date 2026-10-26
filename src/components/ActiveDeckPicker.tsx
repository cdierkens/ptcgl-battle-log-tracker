import { useId } from "react";

import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { cn } from "@/lib/utils";

import type { Deck } from "../lib/tauri";

import { deckLabel } from "../lib/deck-label";
import { SELECT_CLASS } from "../lib/form-styles";
import { DeckOptions } from "./DeckOptions";

interface ActiveDeckPickerProps {
  activeDeck: Deck | null;
  decks: Deck[] | null;
  disabled?: boolean;
  error?: null | string;
  onChange: (id: null | string) => void;
}

/** One active deck for the app; both windows share the Decks tab. */
export function ActiveDeckPicker({
  activeDeck,
  decks,
  disabled = false,
  error = null,
  onChange,
}: ActiveDeckPickerProps) {
  const id = useId();
  const loading = decks === null && error === null;
  const choices = decks ?? [];
  const value = activeDeck?.id ?? "";
  const summary = activeDeck === null ? null : deckSummary(activeDeck);
  const select = (
    <select
      className={cn(SELECT_CLASS, "h-10 w-full")}
      disabled={disabled || loading}
      id={id}
      onChange={(event) => onChange(event.target.value === "" ? null : event.target.value)}
      value={value}
    >
      <option value="">No deck</option>
      {activeDeck !== null && !choices.some((deck) => deck.id === activeDeck.id) ? (
        <option value={activeDeck.id}>{deckLabel(activeDeck)}</option>
      ) : null}
      <DeckOptions decks={choices} />
    </select>
  );

  return (
    <Field>
      <FieldLabel htmlFor={id}>Active deck</FieldLabel>
      {select}
      <FieldDescription>
        {error !== null
          ? error
          : loading
            ? "Loading decks…"
            : choices.length === 0
              ? "Add a deck to choose one. New matches save without a deck until then."
              : (summary ?? "New matches save without a deck.")}
      </FieldDescription>
    </Field>
  );
}

function deckSummary(deck: Deck): string {
  const parts = [deck.currentVersion.formatLabel, deck.currentVersion.archetype?.label].filter(
    (part): part is string => part !== null && part !== undefined,
  );
  return parts.length === 0 ? deck.name : `${deck.name} · ${parts.join(" · ")}`;
}
