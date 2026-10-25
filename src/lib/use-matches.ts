import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { logSavedState } from "./capture-status";
import { createAnalysisCache, type MatchAnalysis } from "./match-row";
import {
  DECKS_CHANGED_EVENT,
  listLogs,
  listMatchDecks,
  type LogSummary,
  type MatchDeck,
  OVERLAY_STATE_EVENT,
  type OverlayStateEvent,
  readLog,
} from "./tauri";

export interface Match {
  analysis: MatchAnalysis | null;
  deck: MatchDeck | null;
  log: LogSummary;
  raw: null | string;
}

export interface MatchesView {
  deckError: null | string;
  error: null | string;
  matches: Match[] | null;
  refresh: () => void;
  refreshing: boolean;
}

/** Saved logs with their parsed summaries and immutable deck snapshots. */
export function useMatches(playerName: string): MatchesView {
  const [logs, setLogs] = useState<LogSummary[] | null>(null);
  const [matchDecks, setMatchDecks] = useState<ReadonlyMap<string, MatchDeck>>(new Map());
  const [raws, setRaws] = useState<ReadonlyMap<string, string>>(new Map());
  const [error, setError] = useState<null | string>(null);
  const [deckError, setDeckError] = useState<null | string>(null);
  const [refreshing, setRefreshing] = useState(true);
  const requested = useRef(new Set<string>());
  // One cache per hook instance; repeat calls return the same analysis.
  const [analyze] = useState(createAnalysisCache);

  const loadLogs = useCallback(() => {
    listLogs()
      .then((next) => {
        setLogs(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setRefreshing(false));
  }, []);

  const loadDecks = useCallback(() => {
    listMatchDecks()
      .then((decks) => {
        setMatchDecks(new Map(decks.map((deck) => [deck.logFileName, deck])));
        setDeckError(null);
      })
      .catch((err: unknown) => {
        setMatchDecks(new Map());
        setDeckError(String(err));
      });
  }, []);

  const load = useCallback(() => {
    loadLogs();
    loadDecks();
  }, [loadDecks, loadLogs]);

  const refresh = useCallback(() => {
    setRefreshing(true);
    setError(null);
    load();
  }, [load]);

  useEffect(load, [load]);

  useEffect(() => {
    const unlisten = listen<OverlayStateEvent>(OVERLAY_STATE_EVENT, (event) => {
      if (logSavedState(event.payload.state) || event.payload.state === "Results") refresh();
    });
    unlisten.catch((err: unknown) => setError(String(err)));
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [refresh]);

  useEffect(() => {
    const unlisten = listen(DECKS_CHANGED_EVENT, loadDecks);
    unlisten.catch((err: unknown) => setDeckError(String(err)));
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [loadDecks]);

  useEffect(() => {
    if (logs === null) return;
    for (const log of logs) {
      if (requested.current.has(log.fileName)) continue;
      requested.current.add(log.fileName);
      readLog(log.fileName)
        .then((raw) => setRaws((current) => new Map(current).set(log.fileName, raw)))
        .catch(() => requested.current.delete(log.fileName));
    }
  }, [logs]);

  const matches = useMemo(() => {
    if (logs === null) return null;
    return logs.map((log) => {
      const raw = raws.get(log.fileName) ?? null;
      return {
        analysis: raw === null ? null : analyze(log.fileName, raw, playerName),
        deck: matchDecks.get(log.fileName) ?? null,
        log,
        raw,
      };
    });
  }, [analyze, logs, matchDecks, playerName, raws]);

  return { deckError, error, matches, refresh, refreshing };
}
