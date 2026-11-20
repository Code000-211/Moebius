// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import type { Firing } from "../../api/types/automations";
import { FiringRow } from "./FiringRow";

afterEach(cleanup);

const firing = (over: Partial<Firing>): Firing => ({
  id: 1,
  status: "started",
  run_id: 9,
  chat_status: null,
  manual: false,
  reason: "",
  at: 0,
  ...over,
});

function show(over: Partial<Firing>) {
  render(
    <MemoryRouter>
      <FiringRow firing={firing(over)} now={60} />
    </MemoryRouter>,
  );
}

describe("a past run", () => {
  it("is done when its chat succeeded, not still 'started'", () => {
    show({ chat_status: "succeeded" });
    expect(screen.getByText("done")).toBeTruthy();
    expect(screen.queryByText("started")).toBeNull();
  });

  it("is told by what its chat became when that went wrong, and 'started' while it runs", () => {
    show({ chat_status: "failed" });
    expect(screen.getByText("failed")).toBeTruthy();
    cleanup();
    show({ chat_status: "running" });
    expect(screen.getByText("started")).toBeTruthy();
  });
});
