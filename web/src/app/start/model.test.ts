import { describe, expect, it } from "vitest";
import { context, fix } from "./model";

describe("Get started", () => {
  it("sends each step to the page that fixes it", () => {
    expect(fix("upstream")).toEqual({ to: "/models", label: "Open Models" });
    expect(fix("pi_bolt")).toEqual({ to: "/settings", label: "Open Settings" });
    expect(fix("coder")).toEqual({ to: "/agents", label: "Open Agents" });
    expect(fix("repository")).toEqual({ to: "/projects/new", label: "Add repository" });
    expect(fix("harness")).toBeNull();
  });

  it("says where the server stands", () => {
    expect(context(undefined)).toBe("Reading what the server still needs.");
    expect(context({ done: true, steps: [], warnings: [] })).toBe(
      "Everything is set up. Cards can run.",
    );
    expect(context({ done: false, steps: [], warnings: [] })).toBe(
      "A few things have to be set up before the first card can run.",
    );
  });
});
