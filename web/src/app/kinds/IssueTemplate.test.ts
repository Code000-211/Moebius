import { describe, expect, it } from "vitest";
import type { Kind } from "../../api/types/kinds";
import { fieldValues } from "./IssueTemplate";

const bug: Kind = {
  id: 1,
  name: "Bug",
  projects: [],
  fields: [
    { id: 1, name: "Severity", type: "choice", options: ["low", "high"] },
    { id: 2, name: "Estimate", type: "number", options: [] },
    { id: 3, name: "Found in", type: "text", options: [] },
  ],
};

describe("fieldValues", () => {
  it("sends the fields that are filled, a number field's value as a number", () => {
    expect(
      fieldValues(bug, {
        title: "",
        body: "",
        fields: { Severity: "high", Estimate: " 3 ", "Found in": "" },
      }),
    ).toEqual({ Severity: "high", Estimate: 3 });
  });

  it("leaves a number the server should refuse as the text typed", () => {
    expect(fieldValues(bug, { title: "", body: "", fields: { Estimate: "many" } })).toEqual({
      Estimate: "many",
    });
  });

  it("sends none for an empty form", () => {
    expect(fieldValues(bug, { title: "", body: "", fields: {} })).toEqual({});
  });
});
