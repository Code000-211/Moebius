import { beforeEach, describe, expect, it } from "vitest";
import { readIds, withRead } from "./read";

beforeEach(() => window.localStorage.clear());

describe("withRead", () => {
  it("adds an id as the newest and keeps it once", () => {
    expect(withRead([1, 2], 3)).toEqual([1, 2, 3]);
    expect(withRead([1, 2, 3], 1)).toEqual([2, 3, 1]);
  });

  it("drops the oldest ids past the limit", () => {
    const many = Array.from({ length: 500 }, (_, id) => id);
    const next = withRead(many, 900);
    expect(next).toHaveLength(500);
    expect(next[0]).toBe(1);
    expect(next.at(-1)).toBe(900);
  });
});

describe("readIds", () => {
  it("is empty when nothing was kept or what was kept is not a list", () => {
    expect(readIds()).toEqual([]);
    window.localStorage.setItem("moebius.inbox.read", "not json");
    expect(readIds()).toEqual([]);
    window.localStorage.setItem("moebius.inbox.read", '{"a":1}');
    expect(readIds()).toEqual([]);
  });

  it("reads the numbers of a kept list", () => {
    window.localStorage.setItem("moebius.inbox.read", '[3,"x",4]');
    expect(readIds()).toEqual([3, 4]);
  });
});
