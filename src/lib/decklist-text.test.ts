import { describe, expect, it } from "vitest";

import { hasDecklistText, isBlank } from "./decklist-text";

describe("decklist text", () => {
  it("treats whitespace as empty", () => {
    expect(isBlank("  \n")).toBe(true);
    expect(hasDecklistText("  \n")).toBe(false);
  });

  it("accepts a card line", () => {
    expect(isBlank("4 Ultra Ball SVI 196")).toBe(false);
    expect(hasDecklistText("4 Ultra Ball SVI 196")).toBe(true);
  });
});
