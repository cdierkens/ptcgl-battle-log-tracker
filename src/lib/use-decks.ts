import { useCallback, useEffect, useState } from "react";

import { archiveDeck, type Deck, listDecks, saveDeck, type SaveDeckInput } from "./tauri";

export interface DecksView {
  archive: (id: string) => Promise<boolean>;
  decks: Deck[] | null;
  error: null | string;
  refresh: () => void;
  save: (input: SaveDeckInput) => Promise<Deck | null>;
  saving: boolean;
}

/** The deck library, loaded once and kept in sync after each save or archive. */
export function useDecks(): DecksView {
  const [decks, setDecks] = useState<Deck[] | null>(null);
  const [error, setError] = useState<null | string>(null);
  const [saving, setSaving] = useState(false);

  const refresh = useCallback(() => {
    listDecks()
      .then((next) => {
        setDecks(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, []);

  useEffect(refresh, [refresh]);

  const save = useCallback(async (input: SaveDeckInput) => {
    setSaving(true);
    setError(null);
    try {
      const saved = await saveDeck(input);
      setDecks((current) => {
        if (current === null) return [saved];
        return [saved, ...current.filter((deck) => deck.id !== saved.id)];
      });
      return saved;
    } catch (err: unknown) {
      setError(String(err));
      return null;
    } finally {
      setSaving(false);
    }
  }, []);

  const archive = useCallback(async (id: string) => {
    setSaving(true);
    setError(null);
    try {
      await archiveDeck(id);
      setDecks((current) => current?.filter((deck) => deck.id !== id) ?? current);
      return true;
    } catch (err: unknown) {
      setError(String(err));
      return false;
    } finally {
      setSaving(false);
    }
  }, []);

  return { archive, decks, error, refresh, save, saving };
}
