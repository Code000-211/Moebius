import { describe, expect, it } from "vitest";
import { fileRequest, size, skillRequest } from "./model";

describe("saving a skill", () => {
  it("sends the trimmed name and description and the text without a second final line break", () => {
    expect(skillRequest({ name: " pdf ", description: " Fill forms. ", text: "Body\n" })).toEqual({
      name: "pdf",
      description: "Fill forms.",
      text: "Body",
    });
  });

  it("says what is wrong with the name and the description", () => {
    expect(skillRequest({ name: "no spaces", description: "x", text: "" })).toBe(
      "A name is made of letters, digits, - and _.",
    );
    expect(skillRequest({ name: "ok", description: "  ", text: "" })).toBe(
      "Enter the description.",
    );
  });
});

describe("saving one of a skill's files", () => {
  it("sends the path inside the skill and the text", () => {
    expect(fileRequest(" /scripts/fill.py ", "print()")).toEqual({
      path: "scripts/fill.py",
      text: "print()",
    });
  });

  it.each([
    ["", "Enter the file's path inside the skill, such as scripts/fill.py."],
    ["../up.txt", "Enter the file's path inside the skill, such as scripts/fill.py."],
    ["a//b", "Enter the file's path inside the skill, such as scripts/fill.py."],
    ["SKILL.md", "SKILL.md is saved with the skill."],
  ])("refuses the path %j", (path, problem) => {
    expect(fileRequest(path, "x")).toBe(problem);
  });

  it("refuses a file that is not text", () => {
    expect(fileRequest("logo.png", null)).toBe("This file is not text and cannot be edited here.");
  });
});

describe("a size", () => {
  it("reads in bytes, kilobytes and megabytes", () => {
    expect([size(12), size(2048), size(3 * 1024 * 1024)]).toEqual(["12 B", "2.0 KB", "3.0 MB"]);
  });
});
