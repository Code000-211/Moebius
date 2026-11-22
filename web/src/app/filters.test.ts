/**
 * What the pages that filter with the shared bar keep and let through: the Fleet's runs and the scope the Usage and Decisions pages send to the server.
 */
import { describe, expect, it } from "vitest";
import { scopeQuery } from "../api/calls/usage";
import { filterFields as fleetFields, filtered as fleetFiltered, type Work } from "./fleet/model";
import { repositoryOf } from "./scope";

const work = (run: number, agent: string, kind: string, repository: string): Work => ({
  run,
  agent,
  kind,
  status: "running",
  asks: false,
  title: `#${run}`,
  id: `#${run}`,
  approve: null,
  repository,
  startedAt: 0,
  tokens: null,
  open: { kind: "chat", chat: run },
});

describe("the fleet's filters", () => {
  const list = [
    work(1, "Coder", "code", "demo"),
    work(2, "Reviewer", "review", "demo"),
    work(3, "Moebius", "chat", "other"),
  ];

  it("offer what the working runs have, and a value that is set even when nothing has it", () => {
    const fields = fleetFields(list, { agent: "Gone" });
    expect(fields.map((field) => field.key)).toEqual(["project", "agent", "kind"]);
    expect(fields[0]?.options).toEqual([
      ["demo", "demo"],
      ["other", "other"],
    ]);
    expect(fields[1]?.options.map(([value]) => value)).toEqual([
      "Coder",
      "Gone",
      "Moebius",
      "Reviewer",
    ]);
  });

  it("let through the runs that pass every filter that is set", () => {
    expect(fleetFiltered(list, {}).map((each) => each.run)).toEqual([1, 2, 3]);
    expect(fleetFiltered(list, { project: "demo" }).map((each) => each.run)).toEqual([1, 2]);
    expect(
      fleetFiltered(list, { project: "demo", agent: "Coder" }).map((each) => each.run),
    ).toEqual([1]);
    expect(fleetFiltered(list, { kind: "chat", project: "demo" })).toEqual([]);
  });
});

describe("the scope of usage and of the decisions", () => {
  it("reads a repository from the address only when it is a number", () => {
    expect(repositoryOf("12")).toBe(12);
    expect(repositoryOf("")).toBeNull();
    expect(repositoryOf("demo")).toBeNull();
  });

  it("is sent as the parameters of the route, left out when not set", () => {
    expect(scopeQuery({}).toString()).toBe("");
    expect(scopeQuery({ repository: 3, agent: "Code r" }).toString()).toBe(
      "repository=3&agent=Code+r",
    );
  });
});
