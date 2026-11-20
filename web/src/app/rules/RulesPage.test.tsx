/** The Rules page as one list of 36px rows grouped by set; a row opens its rule in the editor window. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RulesPage } from "./RulesPage";

const VIEW = {
  patterns: [{ name: "no-force-push", description: "Never force-push." }],
  prose: {
    source: "harness",
    editable: true,
    text: "## be-careful\nAsk first.\n",
    rules: [{ name: "be-careful", text: "Ask first." }],
    error: null,
  },
  repositories: [
    {
      repository: "api",
      repository_id: 7,
      text: "## Tests\nRun them.\n",
      rules: [{ name: "Tests", text: "Run them." }],
      error: null,
    },
  ],
  refusals: { "no-force-push": 2 },
};

function show() {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({ url, init });
      return { ok: true, status: 200, text: async () => JSON.stringify(VIEW) };
    }),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={["/rules"]}>
        <RulesPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return calls;
}

afterEach(() => vi.unstubAllGlobals());

describe("the rules as a list of rows", () => {
  it("lists every set with its rules and has no properties column", async () => {
    show();
    expect(await screen.findByText("no-force-push")).toBeTruthy();
    expect(screen.getByText("be-careful")).toBeTruthy();
    expect(screen.getByText("Tests")).toBeTruthy();
    expect(screen.getByText("2 refusals")).toBeTruthy();
    expect(screen.queryByRole("region", { name: /Properties of/ })).toBeNull();
    expect(screen.queryByTestId("editor-window")).toBeNull();
  });

  it("opens the editor window on a row and asks before Escape drops a change", async () => {
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Edit rule be-careful" }));
    const window = screen.getByTestId("editor-window");
    fireEvent.change(within(window).getByLabelText("Rule text"), {
      target: { value: "Ask twice." },
    });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("alertdialog", { name: "Discard changes?" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    await waitFor(() => expect(screen.queryByTestId("editor-window")).toBeNull());
  });

  it("saves the whole file and closes the window", async () => {
    const calls = show();
    fireEvent.click(await screen.findByRole("button", { name: "Edit rule be-careful" }));
    fireEvent.change(screen.getByLabelText("Rule text"), { target: { value: "Ask twice." } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByTestId("editor-window")).toBeNull());
    const put = calls.find((call) => call.init?.method === "PUT");
    expect(put?.url).toContain("/v1/rules");
    expect(String(put?.init?.body)).toContain("Ask twice.");
  });

  it("adds a rule from the last row", async () => {
    show();
    const rows = await screen.findAllByRole("button", { name: "Add a rule" });
    fireEvent.click(rows[0] as HTMLElement);
    expect(screen.getByRole("dialog", { name: "New rule" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Save" }).getAttribute("disabled")).not.toBeNull();
  });
});
