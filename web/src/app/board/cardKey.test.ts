import { describe, expect, it } from "vitest";
import { cardKey, cardName, parseCardKey } from "./cardKey";

const keyed = { number: 17, project_number: 24, project_key: "WEB" };

describe("how a card is named", () => {
  it("is the project's key and the card's number in the project", () => {
    expect(cardKey(keyed)).toBe("WEB-24");
  });

  it("shows the forge's number beside the key for a card of a forge", () => {
    expect(cardName(keyed, true)).toBe("WEB-24 · #17");
    expect(cardName(keyed, false)).toBe("WEB-24");
  });

  it("falls back to the issue number from a server that sends no key", () => {
    expect(cardKey({ number: 17 })).toBe("#17");
    expect(cardName({ number: 17 }, true)).toBe("#17");
  });

  it("reads a key in any case and refuses what is not one", () => {
    expect(parseCardKey("web-24")).toEqual({ key: "WEB", number: 24 });
    expect(parseCardKey(" WEB-24 ")).toEqual({ key: "WEB", number: 24 });
    for (const not of ["#24", "24", "WEB", "WEB-", "W-1", "WEB-1-2", "WEB-x", "1WEB-1"]) {
      expect(parseCardKey(not), not).toBeNull();
    }
  });
});
