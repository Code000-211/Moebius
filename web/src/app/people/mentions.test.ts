import { describe, expect, it } from "vitest";
import type { Person } from "../../api/types/server";
import { completed, OFFERED, offered, typing } from "./mentions";

const person = (handle: string, name: string, disabled = false): Person => ({
  id: 1,
  handle,
  name,
  role: "member",
  disabled,
  created_at: 0,
});

describe("typing", () => {
  it("finds the handle being typed after an @ that starts a word", () => {
    expect(typing("@", 1)).toEqual({ at: 0, query: "" });
    expect(typing("Ask @le", 7)).toEqual({ at: 4, query: "le" });
    expect(typing("Ask (@le", 8)).toEqual({ at: 5, query: "le" });
    expect(typing("a\n@t-o_1", 8)).toEqual({ at: 2, query: "t-o_1" });
  });

  it("finds nothing in an address, after a space, or when the caret is elsewhere", () => {
    expect(typing("me@example", 10)).toBeNull();
    expect(typing("Ask @lea now", 12)).toBeNull();
    expect(typing("@@lea", 5)).toBeNull();
    expect(typing("no at", 5)).toBeNull();
    expect(typing("Ask @lea", 3)).toBeNull();
  });
});

describe("offered", () => {
  const people = [
    person("lea", "Lea Kim"),
    person("tom", "Tom Lee"),
    person("ada", "Ada", true),
    person("ben", "Ben Lea"),
  ];

  it("lists handles that start with the query before names that do, and never a disabled person", () => {
    expect(offered(people, "le").map((p) => p.handle)).toEqual(["lea", "tom", "ben"]);
    expect(offered(people, "").map((p) => p.handle)).toEqual(["lea", "tom", "ben"]);
    expect(offered(people, "ad")).toEqual([]);
    expect(offered(people, "LEA").map((p) => p.handle)).toEqual(["lea", "ben"]);
  });

  it("offers at most a few", () => {
    const many = Array.from({ length: 20 }, (_, i) => person(`p${i}`, "x"));
    expect(offered(many, "p")).toHaveLength(OFFERED);
  });
});

describe("completed", () => {
  it("writes the handle in place of what was typed and puts the caret after it", () => {
    expect(completed("Ask @le now", 4, 7, "lea")).toEqual({
      text: "Ask @lea now",
      caret: 9,
    });
    expect(completed("@", 0, 1, "tom")).toEqual({ text: "@tom ", caret: 5 });
  });
});
