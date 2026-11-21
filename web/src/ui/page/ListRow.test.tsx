import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { fitId, idChars, ListRow } from "./ListRow";

describe("the id column", () => {
  it("is sized to the longest id, between a narrowest and a widest", () => {
    expect(idChars([])).toBe(8);
    expect(idChars(["#12", "MOE-1"])).toBe(8);
    expect(idChars(["MOEBIUS-123", "MOEBIUS-9"])).toBe(11);
    expect(idChars(["AVERYLONGPROJECTKEY-1234"])).toBe(16);
  });

  it("shows the number of a long key whole and cuts the project part", () => {
    expect(fitId("MOEBIUS-123", 11)).toBe("MOEBIUS-123");
    expect(fitId("AVERYLONGPROJECTKEY-1234", 12)).toBe("AVERYL…-1234");
    expect(fitId("AVERYLONGPROJECTKEY-1234", 5)).toBe("…-1234");
    expect(fitId("#123", 8)).toBe("#123");
  });

  it("draws MOEBIUS-123 whole when the list asks for room for it", () => {
    render(<ListRow id="MOEBIUS-123" idChars={idChars(["MOEBIUS-123"])} title="Fix it" />);
    expect(screen.getByText("MOEBIUS-123")).toBeTruthy();
  });
});
