import { describe, expect, it } from "vitest";
import type { RulesView } from "../../api/types/rules";
import { chosenScope, scopeKey, scopeName } from "./scopes";

const view = (serverRules: number): RulesView =>
  ({
    patterns: [],
    prose: { rules: Array.from({ length: serverRules }, (_, n) => ({ name: `r${n}`, text: "" })) },
    repositories: [{ repository: "api", repository_id: 7, rules: [] }],
    refusals: {},
  }) as unknown as RulesView;

const at = (query: string, v: RulesView) => scopeKey(chosenScope(new URLSearchParams(query), v));

describe("the scope the address chooses", () => {
  it("is the server's when it has rules and the built-in patterns when it has none", () => {
    expect(at("", view(2))).toBe("server");
    expect(at("", view(0))).toBe("builtin");
  });

  it("reads each scope and falls back for one the page does not know", () => {
    expect(at("scope=builtin", view(2))).toBe("builtin");
    expect(at("scope=refusals", view(2))).toBe("refusals");
    expect(at("scope=repository-7", view(2))).toBe("repository-7");
    expect(at("scope=repository-9", view(2))).toBe("server");
  });

  it("still reads the addresses of the earlier page", () => {
    expect(at("scope=patterns", view(2))).toBe("builtin");
    expect(at("project=7", view(2))).toBe("repository-7");
  });

  it("names a scope", () => {
    expect(scopeName({ kind: "repository", id: 7 }, view(0))).toBe("api");
    expect(scopeName({ kind: "server" }, view(0))).toBe("Server");
  });
});
