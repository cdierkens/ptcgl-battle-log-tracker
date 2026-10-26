import { describe, expect, it } from "vitest";

import { hasDecklistText } from "./decklist-text";

describe("hasDecklistText", () => {
  it("treats whitespace as empty", () => {
    expect(hasDecklistText("  \n")).toBe(false);
  });

  it("accepts a card line", () => {
    expect(hasDecklistText("4 Ultra Ball SVI 196")).toBe(true);
  });
});
