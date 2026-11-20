// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Avatar, AvatarStack } from "./Avatar";

afterEach(cleanup);

describe("an avatar", () => {
  it("is a person's initial in white on an avatar colour, the same colour for the same handle", () => {
    render(<Avatar handle="leon" />);
    const mark = screen.getByTitle("@leon");
    expect(mark.textContent).toBe("l");
    expect(mark.className).toMatch(/bg-avatar-[123]/);
    expect(mark.className).toContain("text-white");
    expect(mark.className).toContain("rounded-full");
    const colour = mark.className.match(/bg-avatar-[123]/)?.[0];
    cleanup();
    render(<Avatar handle="Leon" />);
    expect(screen.getByTitle("@Leon").className).toContain(colour);
  });

  it("is the dark indigo rounded square for Moebius and its agents", () => {
    render(<Avatar handle="agent:Coder" />);
    const mark = screen.getByTitle("Coder");
    expect(mark.className).toContain("bg-avatar-agent");
    expect(mark.className).toContain("rounded-control");
    expect(mark.className).not.toContain("rounded-full");
  });

  it("stacks the first three and counts the rest", () => {
    render(<AvatarStack handles={["a", "b", "c", "d", "e"]} />);
    expect(screen.getByRole("img").getAttribute("aria-label")).toContain("@e");
    expect(screen.getByText("+2")).toBeTruthy();
  });
});
