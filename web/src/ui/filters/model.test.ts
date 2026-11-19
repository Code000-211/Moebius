import { describe, expect, it } from "vitest";
import {
  activeFilters,
  displayQuery,
  type FilterField,
  hasDisplay,
  parseViews,
  readDisplay,
  readFilters,
  toggled,
  viewQuery,
  withView,
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

describe("the views kept in a browser", () => {
  it("reads a list of views and nothing else", () => {
    expect(parseViews(null)).toEqual([]);
    expect(parseViews("not json")).toEqual([]);
    expect(parseViews('{"name":"a"}')).toEqual([]);
    expect(parseViews('[{"name":"A","query":"x=1"},{"name":3}]')).toEqual([
      { name: "A", query: "x=1" },
    ]);
  });

  it("names a new view after how many there are", () => {
    const one = withView([], "status=held");
    expect(one).toEqual([{ name: "View 1", query: "status=held" }]);
    expect(withView(one, "layout=list")[1]).toEqual({ name: "View 2", query: "layout=list" });
  });
});

describe("a saved view", () => {
  it("holds only the parameters of the bar that are set", () => {
    const params = new URLSearchParams("card=4&status=held&layout=list&column=");
    expect(viewQuery(params, ["column", "status", "layout"])).toBe("status=held&layout=list");
  });
});
