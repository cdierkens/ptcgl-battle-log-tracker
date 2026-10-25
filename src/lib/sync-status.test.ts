import { describe, expect, it } from "vitest";

import type { Destination, SyncStatus } from "./tauri";

import { attentionReason, matchSyncs, syncActions, syncNeedsAttention } from "./sync-status";

function destination(id: string, name: string, enabled = true): Destination {
  return {
    enabled,
    headers: {},
    id,
    name,
    send_deck_identity: false,
    token_url: null,
    url: `https://${id}.example/api`,
  };
}

const dojo = destination("dojo", "PokeDojo");
const court = destination("court", "TrainingCourt");

function status(deliveries: [string, string, "failed" | "pending" | "synced"][], attention: string[] = []): SyncStatus {
  return {
    attention: attention.map((d) => ({ destination: d, reason: "The site rejected the token (HTTP 401)." })),
    deliveries: deliveries.map(([log, destination, state]) => ({ destination, lastError: null, log, state })),
  };
}

describe("syncActions", () => {
  it("offers each enabled destination, including for an older match", () => {
    expect(syncActions(status([]), "old.txt", [dojo, court, destination("off", "Off", false)])).toEqual([
      { destinationId: "dojo", label: "Sync to PokeDojo" },
      { destinationId: "court", label: "Sync to TrainingCourt" },
    ]);
  });

  it("reads Retry sync when that delivery failed", () => {
    const s = status([["a.txt", "dojo", "failed"]]);
    expect(syncActions(s, "a.txt", [dojo])).toEqual([{ destinationId: "dojo", label: "Retry sync" }]);
    expect(syncActions(s, "a.txt", [dojo, court])[0]?.label).toBe("Retry sync to PokeDojo");
  });
});

describe("attention", () => {
  it("reports why a destination is paused", () => {
    const s = status([], ["dojo"]);
    expect(attentionReason(s, "dojo")).toMatch(/rejected/);
    expect(attentionReason(s, "court")).toBeNull();
  });

  it("badges only for a configured destination", () => {
    expect(syncNeedsAttention(status([], ["dojo"]), [dojo])).toBe(true);
    expect(syncNeedsAttention(status([], ["gone"]), [dojo])).toBe(false);
    expect(syncNeedsAttention(null, [dojo])).toBe(false);
  });
});

describe("matchSyncs", () => {
  const states = (s: null | SyncStatus, log: string, destinations: Destination[]) =>
    matchSyncs(s, log, destinations).map((m) => [m.destination.id, m.state]);

  it("covers every configured destination on every match, in Settings order", () => {
    const off = destination("off", "Off", false);
    const s = status([
      ["a.txt", "court", "failed"],
      ["a.txt", "gone", "synced"],
      ["b.txt", "dojo", "pending"],
    ]);
    expect(states(s, "a.txt", [dojo, court, off])).toEqual([
      ["dojo", "unsynced"],
      ["court", "failed"],
      ["off", "unsynced"],
    ]);
  });

  it("reads unsynced when nothing was ever queued", () => {
    expect(states(null, "a.txt", [dojo])).toEqual([["dojo", "unsynced"]]);
  });

  it("carries the delivery's last error", () => {
    const s: SyncStatus = {
      attention: [],
      deliveries: [{ destination: "dojo", lastError: "HTTP 503", log: "a.txt", state: "pending" }],
    };
    expect(matchSyncs(s, "a.txt", [dojo])[0]?.lastError).toBe("HTTP 503");
  });
});
