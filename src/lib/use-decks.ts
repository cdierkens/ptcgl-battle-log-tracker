import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

import {
  archiveDeck,
  type Deck,
  DECKS_CHANGED_EVENT,
  getActiveDeck,
  listDecks,
  saveDeck,
  type SaveDeckInput,
  setActiveDeck as setActiveDeckCommand,
} from "./tauri";

export interface DecksView {
  activeDeck: Deck | null;
  archive: (id: string) => Promise<boolean>;
  decks: Deck[] | null;
  error: null | string;
  refresh: () => void;
  save: (input: SaveDeckInput) => Promise<Deck | null>;
  saving: boolean;
  setActive: (id: null | string) => Promise<boolean>;
}

/** The deck library and its one active deck, kept in sync across windows. */
export function useDecks(): DecksView {
  const [decks, setDecks] = useState<Deck[] | null>(null);
  const [activeDeck, setActive] = useState<Deck | null>(null);
  const [error, setError] = useState<null | string>(null);
  const [saving, setSaving] = useState(false);

  const load = useCallback(() => {
    Promise.all([listDecks(), getActiveDeck()])
      .then(([next, active]) => {
        setDecks(next);
        setActive(active);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, []);

  useEffect(load, [load]);

  useEffect(() => {
    const unlisten = listen(DECKS_CHANGED_EVENT, load);
    unlisten.catch((err: unknown) => setError(String(err)));
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [load]);

  const save = useCallback(async (input: SaveDeckInput) => {
    setSaving(true);
    setError(null);
    try {
      const saved = await saveDeck(input);
      setDecks((current) => {
        if (current === null) return [saved];
        return [saved, ...current.filter((deck) => deck.id !== saved.id)];
      });
      setActive((current) => (current?.id === saved.id ? saved : current));
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
      setActive((current) => (current?.id === id ? null : current));
      return true;
    } catch (err: unknown) {
      setError(String(err));
      return false;
    } finally {
      setSaving(false);
    }
  }, []);

  const setActiveDeck = useCallback(async (id: null | string) => {
    setSaving(true);
    setError(null);
    try {
      await setActiveDeckCommand(id);
      setActive(await getActiveDeck());
      return true;
    } catch (err: unknown) {
      setError(String(err));
      return false;
    } finally {
      setSaving(false);
    }
  }, []);

  return { activeDeck, archive, decks, error, refresh: load, save, saving, setActive: setActiveDeck };
}
