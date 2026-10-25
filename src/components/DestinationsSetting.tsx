import { useState } from "react";

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
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldLabel, FieldTitle } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";

import type { SyncStatus } from "../lib/tauri";
import type { SettingsView } from "../lib/use-settings";

import { canConfirm, hostOf, tokenToSend } from "../lib/destinations";
import { attentionReason } from "../lib/sync-status";
import {
  type Destination,
  importDestination,
  type ImportPreview,
  openDestinationTokenPage,
  openTokenPage,
  previewDestination,
  removeDestination,
  retryDestination,
  setDestinationDeckIdentity,
  setDestinationEnabled,
  setDestinationToken,
} from "../lib/tauri";

type Confirming = { config: string; preview: ImportPreview; token: string; type: "confirming" };

interface NeedsAttentionProps {
  busy: boolean;
  destination: Destination;
  onError: (err: unknown) => void;
  reason: string;
  run: (action: () => Promise<void>) => void;
}

type Step = Confirming | { config: string; type: "pasting" } | { type: "idle" };

/** Sync destinations: paste a site's config, confirm it, then enable, disable, or remove. */
export function DestinationsSetting({ settings, sync }: { settings: SettingsView; sync: null | SyncStatus }) {
  const destinations = settings.settings?.destinations ?? [];
  const [step, setStep] = useState<Step>({ type: "idle" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<null | string>(null);
  const [pendingRemove, setPendingRemove] = useState<Destination | null>(null);

  const run = (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    action()
      .catch((err: unknown) => setError(String(err)))
      .finally(() => setBusy(false));
  };

  const fail = (err: unknown) => setError(String(err));

  const cancel = () => {
    setStep({ type: "idle" });
    setError(null);
  };

  const review = (config: string) =>
    run(async () => {
      const preview = await previewDestination(config);
      setStep({ config, preview, token: "", type: "confirming" });
    });

  const save = ({ config, preview, token }: Confirming) =>
    run(async () => {
      await importDestination(config, tokenToSend(preview, token));
      setStep({ type: "idle" });
    });

  const openPage = (config: string) => {
    setError(null);
    openTokenPage(config).catch((err: unknown) => setError(String(err)));
  };

  return (
    <Field>
      <FieldTitle>Sync</FieldTitle>
      <FieldDescription>Sends each saved Battle Log to a site you use. Copy the destination config from the site and paste it here.</FieldDescription>

      {destinations.map((destination) => {
        const reason = attentionReason(sync, destination.id);
        return (
        <div className="space-y-2 py-1" key={destination.id}>
        <div className="flex items-center gap-3">
          <Switch
            checked={destination.enabled}
            disabled={busy}
            id={`destination-${destination.id}`}
            onCheckedChange={(enabled) => run(() => setDestinationEnabled(destination.id, enabled))}
          />
          <FieldLabel className="min-w-0 flex-1 flex-col items-start gap-0" htmlFor={`destination-${destination.id}`}>
            <span>{destination.name}</span>
            <span className="text-xs font-normal text-muted-foreground">{hostOf(destination.url)}</span>
          </FieldLabel>
          <Button disabled={busy} onClick={() => setPendingRemove(destination)} size="sm" variant="ghost">
            Remove
          </Button>
        </div>
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <Switch
              checked={destination.send_deck_identity}
              disabled={busy}
              id={`destination-deck-${destination.id}`}
              onCheckedChange={(enabled) => run(() => setDestinationDeckIdentity(destination.id, enabled))}
            />
            <label className="text-xs font-medium" htmlFor={`destination-deck-${destination.id}`}>
              Send deck identity
            </label>
          </div>
          <p className="text-xs text-muted-foreground">
            When on, includes the saved decklist, format, and archetypes with each match. Your decklist is player data;
            this is off by default.
          </p>
        </div>
        {reason === null ? null : (
          <NeedsAttention busy={busy} destination={destination} onError={fail} reason={reason} run={run} />
        )}
        </div>
        );
      })}

      {step.type === "idle" ? (
        <div className="pt-1">
          <Button onClick={() => setStep({ config: "", type: "pasting" })} size="sm" variant="outline">
            Paste config
          </Button>
        </div>
      ) : null}

      {step.type === "pasting" ? (
        <div className="space-y-2">
          <Textarea
            aria-label="Destination config"
            autoFocus
            className="h-24 font-mono text-xs"
            onChange={(event) => setStep({ config: event.target.value, type: "pasting" })}
            placeholder='{"ptcglTrackerDestination": 1}'
            value={step.config}
          />
          <div className="flex gap-2">
            <LoadingButton disabled={step.config.trim() === ""} loading={busy} onClick={() => review(step.config)} size="sm">
              Continue
            </LoadingButton>
            <Button onClick={cancel} size="sm" variant="ghost">
              Cancel
            </Button>
          </div>
        </div>
      ) : null}

      {step.type === "confirming" ? (
        <div className="space-y-3 rounded-md border p-3">
          <p className="text-sm">
            Sync saved Battle Logs to <strong>{step.preview.name}</strong> at{" "}
            <span className="font-mono">{hostOf(step.preview.url)}</span>?
          </p>
          {step.preview.replaces === null ? null : (
            <p className="text-xs text-muted-foreground">This updates {step.preview.replaces}.</p>
          )}
          {step.preview.token === "embedded" ? null : (
            <Field>
              <FieldLabel htmlFor="destination-token">
                {step.preview.token === "required" ? "Token" : "New token (optional)"}
              </FieldLabel>
              <Input
                autoComplete="off"
                id="destination-token"
                onChange={(event) => setStep({ ...step, token: event.target.value })}
                type="password"
                value={step.token}
              />
              <FieldDescription>
                {step.preview.token === "optional" ? "Leave blank to keep the current token. " : null}
                {step.preview.tokenUrl === null ? null : (
                  <Button className="h-auto p-0" onClick={() => openPage(step.config)} size="sm" variant="link">
                    Get a token from {hostOf(step.preview.tokenUrl)}
                  </Button>
                )}
              </FieldDescription>
            </Field>
          )}
          <div className="flex gap-2">
            <LoadingButton
              disabled={!canConfirm(step.preview, step.token)}
              loading={busy}
              onClick={() => save(step)}
              size="sm"
            >
              {step.preview.replaces === null ? "Add" : "Update"}
            </LoadingButton>
            <Button onClick={cancel} size="sm" variant="ghost">
              Cancel
            </Button>
          </div>
        </div>
      ) : null}

      {error !== null ? <StatusAlert tone="critical">{error}</StatusAlert> : null}

      <AlertDialog
        onOpenChange={(open) => {
          if (!open) setPendingRemove(null);
        }}
        open={pendingRemove !== null}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove {pendingRemove?.name}?</AlertDialogTitle>
            <AlertDialogDescription>
              Battle Logs waiting to sync won't be sent, and its token is deleted from this Mac. Matches already synced
              stay on {pendingRemove === null ? "the site" : hostOf(pendingRemove.url)}.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                const target = pendingRemove;
                if (target !== null) run(() => removeDestination(target.id));
              }}
              variant="destructive"
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Field>
  );
}

/** A paused destination: why, plus Paste token and Retry sync. */
function NeedsAttention({ busy, destination, onError, reason, run }: NeedsAttentionProps) {
  const [token, setToken] = useState<null | string>(null);
  const inputId = `destination-token-${destination.id}`;

  if (token !== null) {
    return (
      <Field className="rounded-md border p-3">
        <FieldLabel htmlFor={inputId}>New token for {destination.name}</FieldLabel>
        <Input
          autoComplete="off"
          autoFocus
          id={inputId}
          onChange={(event) => setToken(event.target.value)}
          type="password"
          value={token}
        />
        {destination.token_url === null ? null : (
          <FieldDescription>
            <Button
              className="h-auto p-0"
              onClick={() => openDestinationTokenPage(destination.id).catch(onError)}
              size="sm"
              variant="link"
            >
              Get a token from {hostOf(destination.token_url)}
            </Button>
          </FieldDescription>
        )}
        <div className="flex gap-2">
          <LoadingButton
            disabled={token.trim() === ""}
            loading={busy}
            onClick={() => run(() => setDestinationToken(destination.id, token).then(() => setToken(null)))}
            size="sm"
          >
            Save token
          </LoadingButton>
          <Button onClick={() => setToken(null)} size="sm" variant="ghost">
            Cancel
          </Button>
        </div>
      </Field>
    );
  }

  return (
    <StatusAlert
      actions={
        <>
          <Button disabled={busy} onClick={() => setToken("")} size="sm" variant="outline">
            Paste token
          </Button>
          <Button disabled={busy} onClick={() => run(() => retryDestination(destination.id))} size="sm" variant="ghost">
            Retry sync
          </Button>
        </>
      }
      tone="caution"
    >
      {destination.name} needs attention: {reason} Syncing to it is paused.
    </StatusAlert>
  );
}
