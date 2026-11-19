import { describe, expect, it } from "vitest";
import type { Card } from "../../api/types/board";
import type { Kind } from "../../api/types/kinds";
import type { Project } from "../../api/types/projects";
import type { Person } from "../../api/types/server";
import {
  bodyOf,
  draftOf,
  fieldNames,
  issueDisplayChoices,
  issueFilterFields,
  issueQuery,
  kindsOf,
  sortOf,
} from "./model";

const KINDS: Kind[] = [
  {
    id: 1,
    name: "Bug",
    projects: [1, 2],
    fields: [
      { id: 1, name: "Severity", type: "choice", options: ["low", "high"] },
      { id: 2, name: "Estimate", type: "number", options: [] },
      { id: 3, name: "Reporter", type: "person", options: [] },
    ],
  },
  {
    id: 2,
    name: "Task",
    projects: [2],
    fields: [{ id: 4, name: "estimate", type: "number", options: [] }],
  },
];

const project = (id: number, key: string): Project => ({
  id,
  name: key.toLowerCase(),
  key,
  created_at: 0,
  repositories: [],
  members: [],
  open_cards: 0,
  files: null,
  kinds: [],
  intake: false,
  private: false,
  member: true,
});

describe("the form of a kind", () => {
  it("round-trips a kind, with a choice's options on one line", () => {
    const draft = draftOf(KINDS[0]);
    expect(draft.fields[0]).toEqual({ name: "Severity", type: "choice", options: "low, high" });
    expect(bodyOf(draft)).toEqual({
      name: "Bug",
      fields: [
        { name: "Severity", type: "choice", options: ["low", "high"] },
        { name: "Estimate", type: "number", options: [] },
        { name: "Reporter", type: "person", options: [] },
      ],
    });
  });

  it("trims names, drops empty options and sends no options for a type that has none", () => {
    expect(
      bodyOf({
        name: "  Epic ",
        fields: [
          { name: " Size ", type: "choice", options: " s, ,m ," },
          { name: "Note", type: "text", options: "left over" },
        ],
      }),
    ).toEqual({
      name: "Epic",
      fields: [
        { name: "Size", type: "choice", options: ["s", "m"] },
        { name: "Note", type: "text", options: [] },
      ],
    });
  });
});

describe("what a project offers", () => {
  it("lists the kinds a project picked", () => {
    expect(kindsOf(KINDS, { id: 1 }).map((k) => k.name)).toEqual(["Bug"]);
    expect(kindsOf(KINDS, { id: 2 }).map((k) => k.name)).toEqual(["Bug", "Task"]);
    expect(kindsOf(KINDS, { id: 3 })).toEqual([]);
  });
});

describe("the fields across kinds", () => {
  it("counts a field of the same name once, whatever its case", () => {
    expect(fieldNames(KINDS)).toEqual(["Severity", "Estimate", "Reporter"]);
  });

  it("offers a filter for each field, with the options of a choice, the people and the values cards hold", () => {
    const cards = [
      { fields: { Estimate: 5 } },
      { fields: { Estimate: 2, Severity: "high" } },
    ] as unknown as Card[];
    const people = [
      { handle: "lea", disabled: false },
      { handle: "tom", disabled: true },
    ] as Person[];
    const fields = issueFilterFields(KINDS, [project(1, "WEB")], cards, people, {});
    expect(fields.map((f) => f.key)).toEqual([
      "project",
      "kind",
      "state",
      "field.Severity",
      "field.Estimate",
      "field.Reporter",
    ]);
    const options = (key: string) => fields.find((f) => f.key === key)?.options.map(([v]) => v);
    expect(options("field.Severity")).toEqual(["high", "low"]);
    expect(options("field.Estimate")).toEqual(["2", "5"]);
    expect(options("field.Reporter")).toEqual(["lea"]);
    expect(options("project")).toEqual(["1"]);
  });

  it("keeps a value that is set though no card has it", () => {
    const fields = issueFilterFields(KINDS, [], [], [], { "field.Estimate": "9" });
    expect(fields.find((f) => f.key === "field.Estimate")?.options).toEqual([["9", "9"]]);
  });
});

describe("the Issues page's address", () => {
  it("sends the filters and the sort the server knows", () => {
    const params = new URLSearchParams(
      "project=2&kind=Bug&field.Severity=high&sort=desc:Estimate&layout=list&junk=1",
    );
    expect(issueQuery(params).toString()).toBe(
      "project=2&kind=Bug&field.Severity=high&sort=Estimate&order=desc",
    );
    expect(issueQuery(new URLSearchParams("sort=asc:Estimate")).toString()).toBe("sort=Estimate");
    expect(issueQuery(new URLSearchParams("")).toString()).toBe("");
  });

  it("reads a sort and offers one each way for every field", () => {
    expect(sortOf("desc:Estimate")).toEqual({ field: "Estimate", descending: true });
    expect(sortOf("asc:Due")).toEqual({ field: "Due", descending: false });
    expect(sortOf("")).toBeNull();
    const choices = issueDisplayChoices(KINDS);
    expect(choices.sorts.map(([v]) => v)).toEqual([
      "",
      "asc:Severity",
      "desc:Severity",
      "asc:Estimate",
      "desc:Estimate",
      "asc:Reporter",
      "desc:Reporter",
    ]);
    expect(choices.layouts).toEqual([]);
  });
});
