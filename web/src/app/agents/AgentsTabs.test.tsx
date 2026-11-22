// @vitest-environment jsdom
/** The tabs of the Agents page: each is an address, the chosen one follows the address, and choosing navigates. */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, describe, expect, it } from "vitest";
import { AgentsTabs } from "./AgentsTabs";

afterEach(cleanup);

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname}</p>;
}

function show(at: string) {
  render(
    <MemoryRouter initialEntries={[at]}>
      <Routes>
        {["agents", "agents/skills", "agents/mcp-servers"].map((path) => (
          <Route
            key={path}
            path={path}
            element={
              <>
                <AgentsTabs value={path} />
                <Where />
              </>
            }
          />
        ))}
      </Routes>
    </MemoryRouter>,
  );
}

describe("the tabs of Agents", () => {
  it("are Agents, Skills and MCP servers, with the one of the address chosen", () => {
    show("/agents/skills");
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["Agents", "Skills", "MCP servers"]);
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["false", "true", "false"]);
  });

  it("navigate to the address of the tab chosen", () => {
    show("/agents");
    fireEvent.click(screen.getByRole("tab", { name: "MCP servers" }));
    expect(screen.getByTestId("where").textContent).toBe("/agents/mcp-servers");
    fireEvent.click(screen.getByRole("tab", { name: "Agents" }));
    expect(screen.getByTestId("where").textContent).toBe("/agents");
  });
});
