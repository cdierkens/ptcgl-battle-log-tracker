import { describe, expect, it } from "vitest";

import { analyzeDecklist, deckSource, identityOf, isFormatWindow } from "./deck-library";

const DRAGAPULT = `Pokémon: 1
1 Dragapult ex TWM 130

Trainer: 4
4 Ultra Ball MEG 131

Energy: 2
2 Basic Fire Energy MEE 10

Total Cards: 7`;

function preview(text: string) {
  const result = analyzeDecklist(text);
  if (!result.ok) throw new Error(result.errors.join("\n"));
  return result.preview;
}

describe("analyzeDecklist", () => {
  it("parses, renders, and exposes section counts", () => {
    const result = preview(DRAGAPULT);

    expect(result.decklist).toContain("Dragapult ex");
    expect(result.decklist).toContain("Total Cards: 7");
    expect(result.sections).toEqual([
      { count: 1, section: "pokemon" },
      { count: 4, section: "trainer" },
      { count: 2, section: "energy" },
    ]);
    expect(result.formatCandidates.length).toBeGreaterThan(0);
  });

  it("keeps unresolved cards visible", () => {
    const result = preview("4 Not A Real Card");

    expect(result.unresolved).toBe(1);
    expect(result.warnings.join(" ")).toContain("not in the catalog");
  });

  it("returns parse errors as values", () => {
    const result = analyzeDecklist("");

    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.errors.length).toBeGreaterThan(0);
  });
});

describe("identityOf", () => {
  it("keeps deckId stable and listId different when printings change", async () => {
    const first = await identityOf(preview("4 Acerola's Mischief MEG 113").deck);
    const second = await identityOf(preview("4 Acerola's Mischief ASC 180").deck);

    expect(first.deckId).toBe(second.deckId);
    expect(first.listId).not.toBe(second.listId);
    expect(first.catalogVersion).toBe(second.catalogVersion);
    expect(first.hashVersion).toBe(second.hashVersion);
  });
});

describe("isFormatWindow", () => {
  it("distinguishes a set window from an opaque label", () => {
    expect(isFormatWindow("TEF-30C")).toBe(true);
    expect(isFormatWindow("Expanded")).toBe(false);
  });
});

describe("deckSource", () => {
  it("recognises an export and treats a bare list as typed", () => {
    expect(deckSource(DRAGAPULT)).toBe("ptcgl");
    expect(deckSource("4 Ultra Ball SVI 196\n2 Basic Fire Energy MEE 10")).toBe("typed");
  });
});
