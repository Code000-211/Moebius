import { describe, expect, it } from "vitest";
import { hookAddress } from "./trigger";

describe("hookAddress", () => {
  it("names the project when it is known", () => {
    expect(hookAddress(3, "release")).toBe("/api/hooks/projects/3/release");
    expect(hookAddress(3, "/release")).toBe("/api/hooks/projects/3/release");
  });

  it("never gives the deprecated address without the project", () => {
    expect(hookAddress(undefined, "release")).toBe("/api/hooks/projects/{project}/release");
  });

  it("shows a placeholder for an empty path", () => {
    expect(hookAddress(3, " ")).toBe("/api/hooks/projects/3/{path}");
  });
});
