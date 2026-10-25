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
  variant?: "compact" | "full";
}

/** One active deck for the app; the drawer gets the compact form. */
export function ActiveDeckPicker({
  activeDeck,
  decks,
  disabled = false,
  error = null,
  onChange,
  variant = "full",
}: ActiveDeckPickerProps) {
  const id = useId();
  const loading = decks === null && error === null;
  const choices = decks ?? [];
  const value = activeDeck?.id ?? "";
  const summary = activeDeck === null ? null : deckSummary(activeDeck);
  const select = (
    <select
      className={cn(SELECT_CLASS, variant === "compact" ? "h-8 flex-1 text-xs" : "h-10 w-full")}
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

  if (variant === "compact") {
    return (
      <div className="space-y-1">
        <div className="flex items-center gap-2">
          <label className="shrink-0 text-xs font-medium text-muted-foreground" htmlFor={id}>
            Deck
          </label>
          {select}
        </div>
        {error !== null ? (
          <p className="text-xs text-destructive">{error}</p>
        ) : loading ? (
          <p className="text-xs text-muted-foreground">Loading decks…</p>
        ) : choices.length === 0 ? (
          <p className="text-xs text-muted-foreground">No decks yet. Add one in the main window.</p>
        ) : summary !== null ? (
          <p className="truncate text-xs text-muted-foreground">{summary}</p>
        ) : null}
      </div>
    );
  }

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
