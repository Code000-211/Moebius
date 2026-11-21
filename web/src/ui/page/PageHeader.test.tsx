// @vitest-environment jsdom
/** The right of the header is the same on every page: quiet actions, the gear or menu, Moebius, the primary button. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AgentProvider } from "../../app/shell/AgentContext";
import { PageHeader } from "./PageHeader";

afterEach(cleanup);

const order = () =>
  screen
    .getAllByRole("button")
    .map((button) => button.textContent || button.getAttribute("aria-label"));

describe("the header's buttons", () => {
  it("run actions, menu, Moebius, primary, whatever the page passes", () => {
    render(
      <AgentProvider>
        <PageHeader
          title="Platform"
          primary={<button type="button">New issue</button>}
          menu={<button type="button" aria-label="Project settings" />}
          actions={<button type="button">Filter</button>}
        />
      </AgentProvider>,
    );
    expect(order()).toEqual(["Filter", "Project settings", "Moebius", "New issue"]);
  });

  it("puts Moebius before the primary button when a page has no menu and after its actions", () => {
    render(
      <AgentProvider>
        <PageHeader
          title="Wiki"
          actions={<button type="button">Templates</button>}
          primary={<button type="button">New page</button>}
        />
      </AgentProvider>,
    );
    expect(order()).toEqual(["Templates", "Moebius", "New page"]);
  });
});
