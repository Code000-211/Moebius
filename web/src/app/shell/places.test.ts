import { describe, expect, it } from "vitest";
import type { Place } from "../../api/types/places";
import { moved, placeAt, placePath, samePlace } from "./places";

const pin = (id: number, kind: Place["kind"] = "project", key = ""): Place => ({
  id,
  kind,
  repository_id: 3,
  key,
  title: "t",
});

describe("kept places", () => {
  it("leads a project to its board and a card to its card on the board", () => {
    expect(placePath(pin(1))).toBe("/board/3");
    expect(placePath(pin(1, "card", "9"))).toBe("/board/3?card=9");
  });

  it("knows a project and a card from the address", () => {
    expect(placeAt("/board/3", "")).toEqual({ kind: "project", repository_id: 3 });
    expect(placeAt("/projects/4/", "?x=1")).toEqual({ kind: "project", repository_id: 4 });
    expect(placeAt("/board/3", "?card=9")).toEqual({ kind: "card", key: "9" });
    expect(placeAt("/inbox", "")).toBeNull();
    expect(placeAt("/board/3", "?card=none")).toEqual({ kind: "project", repository_id: 3 });
  });

  it("compares places by kind, repository and key", () => {
    expect(samePlace(pin(1), { kind: "project", repository_id: 3 })).toBe(true);
    expect(samePlace(pin(1), { kind: "project", repository_id: 4 })).toBe(false);
    expect(samePlace(pin(1, "card", "9"), { kind: "card", key: "9" })).toBe(true);
  });

  it("moves a pin one place and stays at the ends", () => {
    const pins = [pin(1), pin(2), pin(3)];
    expect(moved(pins, 2, -1)).toEqual([1, 3, 2]);
    expect(moved(pins, 0, 1)).toEqual([2, 1, 3]);
    expect(moved(pins, 0, -1)).toEqual([1, 2, 3]);
    expect(moved(pins, 2, 1)).toEqual([1, 2, 3]);
  });
});
