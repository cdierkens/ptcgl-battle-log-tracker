import { Cloud, CloudAlert, CloudCheck, CloudUpload, type LucideIcon, MoreHorizontal } from "lucide-react";
import { type ReactNode, useState } from "react";

import { LoadingButton } from "@/components/LoadingButton";
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

import type { Destination, MatchDeck, SyncStatus } from "../lib/tauri";
import type { DecksView } from "../lib/use-decks";
import type { Match, MatchesView } from "../lib/use-matches";

import { matchRow, type MatchRow } from "../lib/match-row";
import { type MatchSync, matchSyncs, type MatchSyncState, syncActions } from "../lib/sync-status";
import { deleteLog, revealLog, syncLog } from "../lib/tauri";
import { useNow } from "../lib/use-now";
import { MatchDeckPanel } from "./MatchDeckPanel";

const RESULT_VARIANT = { "—": "secondary", L: "destructive", W: "default" } as const;

const SYNC_LOOK: Record<MatchSyncState, { icon: LucideIcon; label: string }> = {
  failed: { icon: CloudAlert, label: "Sync failed" },
  pending: { icon: CloudUpload, label: "Waiting to sync" },
  synced: { icon: CloudCheck, label: "Synced" },
  unsynced: { icon: Cloud, label: "Not synced" },
};

interface MatchDetailProps {
  actions: ReactNode;
  deckError: null | string;
  decks: DecksView;
  destinations: Destination[];
  match: Match;
  onBack: () => void;
  onDeckChanged: () => void;
  onOpenDecks?: () => void;
  row: MatchRow;
  sync: null | SyncStatus;
}

interface MatchListProps {
  decks: DecksView;
  destinations: Destination[];
  matches: MatchesView;
  onOpenDecks?: () => void;
  playerName: string;
  sync: null | SyncStatus;
}

export function MatchList({ decks, destinations, matches, onOpenDecks, playerName, sync }: MatchListProps) {
  const [selected, setSelected] = useState<null | string>(null);
  const [pendingDelete, setPendingDelete] = useState<Match | null>(null);
  const [actionError, setActionError] = useState<null | string>(null);
  const now = useNow();

  if (matches.matches === null) {
    return matches.error === null ? (
      <div className="space-y-2 p-4">
        <Skeleton className="h-10 w-full" />
        <Skeleton className="h-10 w-full" />
        <Skeleton className="h-10 w-full" />
      </div>
    ) : (
      <StatusAlert
        actions={
          <LoadingButton loading={matches.refreshing} onClick={matches.refresh} size="sm" variant="outline">
            Try again
          </LoadingButton>
        }
        className="m-4"
        tone="critical"
      >
        Couldn't load saved Battle Logs: {matches.error}
      </StatusAlert>
    );
  }

  const copy = (match: Match) => {
    setActionError(null);
    if (match.raw === null) {
      setActionError("That Battle Log is still loading.");
      return;
    }
    navigator.clipboard.writeText(match.raw).catch((err: unknown) => setActionError(String(err)));
  };

  const reveal = (match: Match) => {
    setActionError(null);
    revealLog(match.log.fileName).catch((err: unknown) => setActionError(String(err)));
  };

  const sendToDestination = (match: Match, destinationId: string) => {
    setActionError(null);
    syncLog(match.log.fileName, destinationId).catch((err: unknown) => setActionError(String(err)));
  };

  const confirmDelete = () => {
    if (pendingDelete === null) return;
    const fileName = pendingDelete.log.fileName;
    setPendingDelete(null);
    deleteLog(fileName)
      .then(() => {
        if (selected === fileName) setSelected(null);
        matches.refresh();
      })
      .catch((err: unknown) => setActionError(String(err)));
  };

  const actions = (match: Match) => (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button aria-label="Battle Log actions" size="icon-sm" variant="ghost">
          <MoreHorizontal aria-hidden />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => copy(match)}>Copy Battle Log</DropdownMenuItem>
        <DropdownMenuItem onSelect={() => reveal(match)}>Show in Finder</DropdownMenuItem>
        {syncActions(sync, match.log.fileName, destinations).map((action) => (
          <DropdownMenuItem key={action.destinationId} onSelect={() => sendToDestination(match, action.destinationId)}>
            {action.label}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => setPendingDelete(match)} variant="destructive">
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  const dialog = (
    <AlertDialog
      onOpenChange={(open) => {
        if (!open) setPendingDelete(null);
      }}
      open={pendingDelete !== null}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete this Battle Log?</AlertDialogTitle>
          <AlertDialogDescription>The file moves to the Trash, so you can put it back from there.</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={confirmDelete} variant="destructive">
            Move to Trash
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );

  const detail = matches.matches.find((match) => match.log.fileName === selected);
  if (detail !== undefined) {
    return (
      <>
        <MatchDetail
          actions={actions(detail)}
          deckError={matches.deckError}
          decks={decks}
          destinations={destinations}
          match={detail}
          onBack={() => setSelected(null)}
          onDeckChanged={matches.refresh}
          onOpenDecks={onOpenDecks}
          row={matchRow(detail.log, detail.analysis, now)}
          sync={sync}
        />
        {actionError !== null ? <StatusAlert className="m-4" tone="critical">{actionError}</StatusAlert> : null}
        {dialog}
      </>
    );
  }

  if (matches.matches.length === 0) {
    return (
      <Empty className="m-4">
        <EmptyHeader>
          <EmptyTitle>No Battle Logs yet</EmptyTitle>
          <EmptyDescription>
            After a match, open Battle Log → Export in the game. Your saved matches show up here.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  const nameMissing = playerName.trim() === "";
  return (
    <div className="space-y-3 p-4">
      {nameMissing ? (
        <StatusAlert tone="info">Add your player name in Settings to see wins and losses.</StatusAlert>
      ) : null}
      {actionError !== null ? <StatusAlert tone="critical">{actionError}</StatusAlert> : null}
      {matches.deckError !== null ? (
        <StatusAlert tone="info">
          Deck snapshots are unavailable. Your saved Battle Logs are unaffected: {matches.deckError}
        </StatusAlert>
      ) : null}
      {matches.error !== null ? <StatusAlert tone="critical">{matches.error}</StatusAlert> : null}
      <ul className="divide-y divide-border rounded-md border border-border">
        {matches.matches.map((match) => {
          const row = matchRow(match.log, match.analysis, now);
          return (
            <li className="flex items-center gap-1 pr-1" key={row.fileName}>
              <Button
                className="h-auto min-w-0 flex-1 justify-start gap-3 rounded-none px-3 py-2 text-left"
                onClick={() => setSelected(row.fileName)}
                variant="ghost"
              >
                <RowContent deck={match.deck} row={row} />
                {matchSyncs(sync, row.fileName, destinations).map((s) => (
                  <RowSyncIcon key={s.destination.id} label={syncLabel(s)} state={s.state} />
                ))}
              </Button>
              {actions(match)}
            </li>
          );
        })}
      </ul>
      {dialog}
    </div>
  );
}

function MatchDetail({
  actions,
  deckError,
  decks,
  destinations,
  match,
  onBack,
  onDeckChanged,
  onOpenDecks,
  row,
  sync,
}: MatchDetailProps) {
  const [showRaw, setShowRaw] = useState(false);
  const syncs = matchSyncs(sync, row.fileName, destinations);
  const summary = match.analysis?.summary;
  return (
    <div className="space-y-3 p-4">
      <div className="flex items-center justify-between">
        <Button onClick={onBack} size="sm" variant="ghost">
          ← All matches
        </Button>
        {actions}
      </div>
      <div className="flex items-center gap-3">
        <RowContent deck={match.deck} row={row} />
      </div>
      {deckError !== null ? (
        <StatusAlert tone="info">Deck snapshots are unavailable: {deckError}</StatusAlert>
      ) : null}
      <MatchDeckPanel decks={decks} match={match} onChanged={onDeckChanged} onOpenDecks={onOpenDecks} />
      {summary === undefined ? (
        <p className="text-sm text-muted-foreground">This Battle Log couldn't be summarized.</p>
      ) : (
        <dl className="grid grid-cols-2 gap-2 text-sm">
          <dt className="text-muted-foreground">Prizes taken</dt>
          <dd className="text-foreground">{row.prizes}</dd>
          <dt className="text-muted-foreground">Knockouts</dt>
          <dd className="text-foreground">
            {summary.knockoutsByPlayer.self}–{summary.knockoutsByPlayer.opponent}
          </dd>
          <dt className="text-muted-foreground">Turns</dt>
          <dd className="text-foreground">{summary.turnCount}</dd>
        </dl>
      )}
      {syncs.length > 0 ? (
        <ul aria-label="Sync" className="space-y-1 text-sm">
          {syncs.map((s) => (
            <li className="flex items-start gap-2" key={s.destination.id}>
              <SyncIcon state={s.state} />
              <span className="min-w-0">
                {syncLabel(s)}
                {s.state !== "synced" && s.lastError !== null ? (
                  <span className="block text-xs text-muted-foreground">{s.lastError}</span>
                ) : null}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      <Button aria-expanded={showRaw} onClick={() => setShowRaw((open) => !open)} size="sm" variant="outline">
        {showRaw ? "Hide raw log" : "View raw log"}
      </Button>
      {showRaw ? (
        <pre className="max-h-72 overflow-y-auto whitespace-pre-wrap rounded-md border border-border p-2 font-mono text-xs text-muted-foreground">
          {match.raw ?? "Loading…"}
        </pre>
      ) : null}
    </div>
  );
}

function RowContent({ deck, row }: { deck: MatchDeck | null; row: MatchRow }) {
  return (
    <>
      <Badge className="w-7 shrink-0 justify-center" variant={row.result === null ? "outline" : RESULT_VARIANT[row.result]}>
        {row.result ?? "?"}
      </Badge>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium text-foreground">{row.title}</span>
        <span className="block truncate text-xs text-muted-foreground">{deck?.deckName ?? "Deck not set"}</span>
        <span className="block text-xs text-muted-foreground">
          {row.prizes === null ? row.when : `Prizes ${row.prizes} · ${row.when}`}
        </span>
      </span>
    </>
  );
}

function RowSyncIcon({ label, state }: { label: string; state: MatchSyncState }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="shrink-0">
          <SyncIcon label={label} state={state} />
        </span>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

/** Pass `label` unless text beside it already says the state. */
function SyncIcon({ label, state }: { label?: string; state: MatchSyncState }) {
  const { icon: Icon } = SYNC_LOOK[state];
  const className = cn("size-4 shrink-0", state === "failed" ? "text-destructive" : "text-muted-foreground");
  return label === undefined ? <Icon aria-hidden className={className} /> : <Icon aria-label={label} className={className} role="img" />;
}

function syncLabel({ destination, state }: MatchSync): string {
  return `${destination.name}: ${SYNC_LOOK[state].label.toLowerCase()}`;
}
