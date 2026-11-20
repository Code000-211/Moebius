import { describe, expect, it } from "vitest";
import { fix } from "./model";

describe("Get started", () => {
  it("sends each step to the page that fixes it", () => {
    expect(fix("upstream")).toEqual({ to: "/models", label: "Open Models" });
    expect(fix("pi_bolt")).toEqual({ to: "/settings", label: "Open Settings" });
    expect(fix("coder")).toEqual({ to: "/agents", label: "Open Agents" });
    expect(fix("repository")).toEqual({ to: "/projects/new", label: "Add repository" });
    expect(fix("harness")).toBeNull();
  });
});
