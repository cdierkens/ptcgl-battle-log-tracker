import { useEffect, useState } from "react";

import { CopyButton } from "@/components/CopyButton";
import { StatusAlert } from "@/components/StatusAlert";
import { Button } from "@/components/ui/button";

import { historyText } from "../lib/capture-status";
import { relativeTime } from "../lib/match-row";
import { lastResultsAt } from "../lib/status-history";
import { type CaptureStatusEvent, getDiagnostics, openScreenRecordingSettings, relaunchApp } from "../lib/tauri";
import { useNow } from "../lib/use-now";

export function HelpTab({ history, state }: { history: CaptureStatusEvent[]; state: null | string }) {
  const [diagnostics, setDiagnostics] = useState<null | string>(null);
  const [error, setError] = useState<null | string>(null);
  const now = useNow();

  useEffect(() => {
    getDiagnostics()
      .then(setDiagnostics)
      .catch((err: unknown) => setError(String(err)));
  }, [state]);

  const lines = history.map((event) => `${new Date(event.ts).toLocaleTimeString()} ${historyText(event)}`);
  const report = diagnostics === null ? "" : [diagnostics, "", "Recent events:", ...lines].join("\n");
  const lastResults = lastResultsAt(history);

  return (
    <div className="space-y-5 p-4">
      {error !== null ? <StatusAlert tone="critical">{error}</StatusAlert> : null}

      <section className="space-y-2">
        <h3 className="text-sm font-semibold text-foreground">Screen Recording</h3>
        {state === "PermissionRequired" ? (
          <StatusAlert
            actions={
              <div className="flex gap-2">
                <Button onClick={() => void openScreenRecordingSettings().catch((err: unknown) => setError(String(err)))} size="sm">
                  Open System Settings
                </Button>
                <Button onClick={() => void relaunchApp()} size="sm" variant="outline">
                  Quit &amp; reopen
                </Button>
              </div>
            }
            tone="critical"
          >
            Needed to spot the results screen. Turn on Battle Log Capture in Privacy &amp; Security → Screen Recording,
            then quit and reopen.
          </StatusAlert>
        ) : (
          <p className="text-sm text-muted-foreground">{state === null ? "Checking…" : "Allowed."}</p>
        )}
      </section>

      <section className="space-y-1">
        <h3 className="text-sm font-semibold text-foreground">Detection</h3>
        <p className="text-sm text-muted-foreground">
          {lastResults === null
            ? "No results screen seen since the app opened."
            : `Last results screen ${relativeTime(lastResults, now)}.`}
        </p>
      </section>

      <section className="space-y-1">
        <h3 className="text-sm font-semibold text-foreground">Decks</h3>
        <p className="text-sm text-muted-foreground">
          Paste a list in the Decks tab and choose an active deck. New saves snapshot its current
          version; editing the deck later does not change old matches. Choose No deck to save
          without one.
        </p>
      </section>

      <section className="space-y-2">
        <h3 className="text-sm font-semibold text-foreground">Recent activity</h3>
        {lines.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nothing yet.</p>
        ) : (
          <ul className="max-h-48 space-y-1 overflow-y-auto rounded-md border border-border p-2 text-xs text-muted-foreground">
            {lines.map((line, index) => (
              <li key={`${history[index].ts}-${index}`}>{line}</li>
            ))}
          </ul>
        )}
      </section>

      <section className="space-y-1">
        <CopyButton disabled={diagnostics === null} label="Copy diagnostic report" size="sm" value={report} variant="outline" />
        <p className="text-xs text-muted-foreground">App state and recent activity only — never your Battle Logs or clipboard.</p>
      </section>
    </div>
  );
}
