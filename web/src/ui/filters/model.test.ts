import { describe, expect, it } from "vitest";
import {
  activeFilters,
  displayQuery,
  type FilterField,
  hasDisplay,
  readDisplay,
  readFilters,
  toggled,
} from "./model";

const fields: FilterField[] = [
  { key: "column", label: "Column", options: [["Done", "Done"]] },
  {
    key: "status",
    label: "Status",
    options: [
      ["held", "Blocked"],
      ["idle", "Idle"],
    ],
  },
];
const choices = {
  groups: [
    ["", "Column"],
    ["agent", "Agent"],
  ] as Array<[string, string]>,
  sorts: [
    ["", "Number"],
    ["title", "Title"],
  ] as Array<[string, string]>,
  fields: [
    ["labels", "Labels"],
    ["description", "Description"],
  ] as Array<[string, string]>,
};

describe("filters in the address", () => {
  it("reads every field, empty when it is not set", () => {
    const params = new URLSearchParams("status=held&other=1");
    expect(readFilters(params, fields)).toEqual({ column: "", status: "held" });
  });

  it("names the filters that are set, by the words of their values", () => {
    const active = activeFilters(fields, { column: "", status: "held" });
    expect(active.map((a) => [a.field.label, a.words])).toEqual([["Status", "Blocked"]]);
  });

  it("keeps a value the field no longer offers, by its own text", () => {
    const active = activeFilters(fields, { column: "Gone", status: "" });
    expect(active.map((a) => a.words)).toEqual(["Gone"]);
  });
});

describe("display in the address", () => {
  it("reads the defaults from an empty address", () => {
    const display = readDisplay(new URLSearchParams(), choices);
    expect(display).toEqual({ layout: "board", group: "", sort: "", hidden: [] });
    expect(hasDisplay(display)).toBe(false);
  });

  it("reads what the page offers and ignores the rest", () => {
    const display = readDisplay(
      new URLSearchParams("layout=list&group=agent&sort=nope&hide=labels,labels,bogus"),
      choices,
    );
    expect(display).toEqual({ layout: "list", group: "agent", sort: "", hidden: ["labels"] });
    expect(hasDisplay(display)).toBe(true);
  });

  it("reads the layouts a page offers, and the timeline only where it is offered", () => {
    const timelines = {
      ...choices,
      layouts: [
        ["list", "List"],
        ["timeline", "Timeline"],
      ] as Array<[string, string]>,
    };
    expect(readDisplay(new URLSearchParams("layout=timeline"), timelines).layout).toBe("timeline");
    expect(readDisplay(new URLSearchParams("layout=list"), timelines).layout).toBe("list");
    expect(readDisplay(new URLSearchParams("layout=board"), timelines).layout).toBe("board");
    expect(readDisplay(new URLSearchParams("layout=timeline"), choices).layout).toBe("board");
  });

  it("writes a default as an empty value, which removes the parameter", () => {
    expect(displayQuery(readDisplay(new URLSearchParams(), choices))).toEqual({
      layout: "",
      group: "",
      sort: "",
      hide: "",
    });
    const written = displayQuery({
      layout: "list",
      group: "agent",
      sort: "title",
      hidden: ["labels", "description"],
    });
    expect(written).toEqual({
      layout: "list",
      group: "agent",
      sort: "title",
      hide: "description,labels",
    });
  });

  it("switches a field off and on", () => {
    expect(toggled([], "labels", false)).toEqual(["labels"]);
    expect(toggled(["labels"], "labels", true)).toEqual([]);
    expect(toggled(["labels"], "description", false)).toEqual(["description", "labels"]);
  });
});
