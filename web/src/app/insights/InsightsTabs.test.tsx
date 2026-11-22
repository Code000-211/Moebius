// @vitest-environment jsdom
/** The tabs of Insights and of Improve: each is an address, the chosen one follows the address, and choosing navigates. */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { ImproveTabs } from "../improve/ImproveTabs";
import { InsightsTabs } from "./InsightsTabs";

afterEach(cleanup);

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname}</p>;
}

function show(at: string, paths: string[], Tabs: (props: { value: string }) => React.ReactNode) {
  render(
    <MemoryRouter initialEntries={[at]}>
      <Routes>
        {paths.map((path) => (
          <Route
            key={path}
            path={path}
            element={
              <>
                <Tabs value={path} />
                <Where />
              </>
            }
          />
        ))}
      </Routes>
    </MemoryRouter>,
  );
}

describe("the tabs of Insights", () => {
  it("are Analytics and Usage, and choosing one navigates to its address", () => {
    show("/insights", ["insights", "insights/usage"], InsightsTabs);
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Analytics", "Usage"]);
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["true", "false"]);
    fireEvent.click(screen.getByRole("tab", { name: "Usage" }));
    expect(screen.getByTestId("where").textContent).toBe("/insights/usage");
  });
});

describe("the tabs of Improve", () => {
  it("are Retro, Lessons, Experiments and Decisions, with the one of the address chosen", () => {
    show(
      "/improve/experiments",
      ["improve", "improve/lessons", "improve/experiments", "improve/decisions"],
      ImproveTabs,
    );
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual([
      "Retro",
      "Lessons",
      "Experiments",
      "Decisions",
    ]);
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual([
      "false",
      "false",
      "true",
      "false",
    ]);
    fireEvent.click(screen.getByRole("tab", { name: "Decisions" }));
    expect(screen.getByTestId("where").textContent).toBe("/improve/decisions");
    fireEvent.click(screen.getByRole("tab", { name: "Retro" }));
    expect(screen.getByTestId("where").textContent).toBe("/improve");
  });
});
