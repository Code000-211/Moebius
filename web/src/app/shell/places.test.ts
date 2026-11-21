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

/** Project 5 is where repository 3 lives; the first repository of project 4 is 3, and project 9 has none. */
const home = (repository: number) => (repository === 3 ? 5 : undefined);
const first = (project: number) => (project === 4 ? 3 : undefined);

describe("kept places", () => {
  it("leads a project to its board and a card to its card on the board", () => {
    expect(placePath(pin(1), home)).toBe("/projects/5?repository=3");
    expect(placePath(pin(1, "card", "9"), home)).toBe("/projects/5?repository=3&card=9");
    expect(placePath(pin(1), () => undefined)).toBeNull();
  });

  it("knows a project and a card from the address", () => {
    expect(placeAt("/projects/4", "", first)).toEqual({ kind: "project", repository_id: 3 });
    expect(placeAt("/projects/4/list", "", first)).toEqual({ kind: "project", repository_id: 3 });
    expect(placeAt("/projects/4/", "?repository=6", first)).toEqual({
      kind: "project",
      repository_id: 6,
    });
    expect(placeAt("/projects/4", "?card=9", first)).toEqual({ kind: "card", key: "9" });
    expect(placeAt("/projects/4", "?card=none", first)).toEqual({
      kind: "project",
      repository_id: 3,
    });
    expect(placeAt("/projects/9", "", first)).toBeNull();
    expect(placeAt("/projects/new", "", first)).toBeNull();
    expect(placeAt("/inbox", "", first)).toBeNull();
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
