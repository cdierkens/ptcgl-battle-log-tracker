import { describe, expect, it } from "vitest";

import {
  analyzeDeckDraft,
  analyzeDecklist,
  deckSource,
  deriveArchetypeLabel,
  findSpecies,
  FORMAT_WINDOWS,
  identityOf,
  isFormatWindow,
  SPECIES,
  speciesLabelFor,
} from "./deck-library";

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

  it("keeps PTCGL's grouped section counts visible as warnings", () => {
    const ptcgl = DRAGAPULT.replace("Trainer: 4", "Trainer: 1").replace("Energy: 2", "Energy: 1");
    const result = preview(ptcgl);

    expect(result.sections).toEqual([
      { count: 1, section: "pokemon" },
      { count: 4, section: "trainer" },
      { count: 2, section: "energy" },
    ]);
    expect(result.warnings.join(" ")).toContain("trainer header declares 1, parsed 4");
    expect(result.warnings.join(" ")).toContain("energy header declares 1, parsed 2");
  });

  it("keeps a genuinely wrong section header visible", () => {
    const result = preview(DRAGAPULT.replace("Trainer: 4", "Trainer: 2"));

    expect(result.warnings.join(" ")).toContain("trainer header declares 2, parsed 4");
  });
});

describe("analyzeDeckDraft", () => {
  it("treats blank text as no list", () => {
    expect(analyzeDeckDraft("  \n")).toEqual({ kind: "empty" });
  });

  it("returns the preview for a valid list", () => {
    const result = analyzeDeckDraft(DRAGAPULT);
    expect(result.kind).toBe("valid");
    if (result.kind === "valid") expect(result.preview.decklist).toContain("Dragapult ex");
  });

  it("returns parser errors for an invalid list", () => {
    const result = analyzeDeckDraft("4");
    expect(result.kind).toBe("invalid");
    if (result.kind === "invalid") expect(result.errors.length).toBeGreaterThan(0);
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

  it("marks blank text as metadata", () => {
    expect(deckSource("  \n")).toBe("metadata");
  });
});

describe("species catalog", () => {
  it("exposes unique labels and slugs", () => {
    expect(SPECIES.length).toBeGreaterThan(0);
    expect(new Set(SPECIES.map((species) => species.slug)).size).toBe(SPECIES.length);
    expect(new Set(SPECIES.map((species) => species.label)).size).toBe(SPECIES.length);
  });

  it("round-trips labels and slugs", () => {
    expect(findSpecies("Dragapult")?.slug).toBe("dragapult");
    expect(findSpecies("dragapult")?.label).toBe("Dragapult");
    expect(speciesLabelFor("excadrill-mega")).toBe("Mega Excadrill");
  });

  it("derives a label from names", () => {
    expect(deriveArchetypeLabel(["Dragapult", "Blaziken"])).toBe("Dragapult / Blaziken");
    expect(deriveArchetypeLabel(["Dragapult", "Dragapult"])).toBe("Dragapult");
    expect(deriveArchetypeLabel([])).toBe("");
  });

  it("exposes the catalog windows", () => {
    expect(FORMAT_WINDOWS).toContain("TEF-30C");
  });
});
