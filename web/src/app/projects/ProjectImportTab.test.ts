import { describe, expect, it } from "vitest";
import { parseUserMap } from "./ProjectImportTab";

describe("parseUserMap", () => {
  it("reads one Jira user and handle on each line", () => {
    expect(parseUserMap("ada@example.com = ada\n\n  Grace Hopper=grace \n")).toEqual({
      map: { "ada@example.com": "ada", "Grace Hopper": "grace" },
      bad: null,
    });
  });

  it("names the first line that is not `user = handle`", () => {
    expect(parseUserMap("ada@example.com = ada\nno-equals-sign").bad).toBe("no-equals-sign");
    expect(parseUserMap("= ada").bad).toBe("= ada");
    expect(parseUserMap("ada =").bad).toBe("ada =");
  });

  it("is empty for no text", () => {
    expect(parseUserMap("")).toEqual({ map: {}, bad: null });
  });
});
