/** The Rules page: a sub-navigation of scopes, the rules of the chosen one as blocks, the editor window and "New rule". */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RulesPage } from "./RulesPage";

const LONG = "Run the whole suite before you push, and never skip a test to get a green run. "
  .repeat(6)
  .trim();

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
      text: `## Tests\n${LONG}\n`,
      rules: [{ name: "Tests", text: LONG }],
      error: null,
    },
    { repository: "web", repository_id: 8, text: "", rules: [], error: null },
  ],
  refusals: { "no-force-push": 2 },
};

function Where() {
  return <output data-testid="where">{useLocation().search}</output>;
}

function show(path = "/rules", view: unknown = VIEW) {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({ url, init });
      return { ok: true, status: 200, text: async () => JSON.stringify(view) };
    }),
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter initialEntries={[path]}>
        <RulesPage />
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return calls;
}

const nav = () => screen.findByRole("navigation", { name: "Rules" });

afterEach(() => vi.unstubAllGlobals());

describe("the sub-navigation", () => {
  it("lists the scopes with their counts, zero included, and Refusals last", async () => {
    show();
    const bar = within(await nav());
    expect(bar.getByRole("link", { name: "Built-in1" })).toBeTruthy();
    expect(bar.getByRole("link", { name: "Server1" })).toBeTruthy();
    expect(bar.getByRole("link", { name: "api1" })).toBeTruthy();
    expect(bar.getByRole("link", { name: "web0" })).toBeTruthy();
    expect(bar.getAllByRole("link").at(-1)?.textContent).toBe("Refusals");
  });

  it("opens on the server when it has rules and on the built-in patterns when it has none", async () => {
    show();
    expect(await screen.findByRole("region", { name: "Server" })).toBeTruthy();
    expect((await nav()).querySelector('[aria-current="page"]')?.textContent).toContain("Server");
    vi.unstubAllGlobals();
    show("/rules", { ...VIEW, prose: { ...VIEW.prose, rules: [], text: "" } });
    expect(await screen.findAllByRole("region", { name: "Built-in" })).toBeTruthy();
  });

  it("shows another scope and keeps it in the address", async () => {
    show();
    fireEvent.click(within(await nav()).getByRole("link", { name: "Built-in1" }));
    expect(await screen.findByRole("region", { name: "Built-in" })).toBeTruthy();
    expect(screen.getByTestId("where").textContent).toBe("?scope=builtin");
  });
});

describe("the rules of a scope", () => {
  it("shows a built-in rule read only, with its text in full and what it refused", async () => {
    show("/rules?scope=builtin");
    const block = await screen.findByRole("article", { name: "no-force-push" });
    expect(within(block).getByText("Never force-push.")).toBeTruthy();
    expect(within(block).getByLabelText("Read only")).toBeTruthy();
    expect(within(block).getByText(/2 refusals/)).toBeTruthy();
    expect(within(block).queryByRole("button")).toBeNull();
  });

  it("shows the whole of a long rule", async () => {
    show("/rules?scope=repository-7");
    const block = await screen.findByRole("article", { name: "Tests" });
    expect(block.textContent).toContain(LONG);
    expect(within(block).getByText("Runs in api", { exact: false })).toBeTruthy();
  });

  it("says an empty scope has no rules and offers to add one", async () => {
    show("/rules?scope=repository-8");
    expect(await screen.findByText("No rules for web.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add a rule" }));
    expect(screen.getByRole("dialog", { name: "New rule" })).toBeTruthy();
  });

  it("lists the rules that refused a call this week under Refusals", async () => {
    show("/rules?scope=refusals");
    const list = await screen.findByRole("region", { name: "Refusals" });
    expect(within(list).getByText("no-force-push")).toBeTruthy();
    expect(within(list).getByText("2 refusals")).toBeTruthy();
    expect(within(list).queryByText("be-careful")).toBeNull();
  });
});

describe("the editor window", () => {
  it("opens on a rule and asks before Escape drops a change", async () => {
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

  it("opens on a press anywhere in the block", async () => {
    show();
    fireEvent.click(await screen.findByText("Ask first."));
    expect(screen.getByTestId("editor-window")).toBeTruthy();
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
});

describe("New rule", () => {
  it("asks for the scope, starting on the one being read, and opens the empty editor there", async () => {
    show("/rules?scope=repository-7");
    fireEvent.click(await screen.findByRole("button", { name: "New rule" }));
    const dialog = screen.getByRole("dialog", { name: "New rule" });
    expect(within(dialog).getByRole("button", { name: "Scope" }).textContent).toContain("api");
    fireEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
    const editor = await screen.findByTestId("editor-window");
    expect(within(editor).getByLabelText("Rule name")).toBeTruthy();
    expect(screen.getByTestId("where").textContent).toBe("?scope=repository-7");
  });

  it("starts on the server when the built-in patterns are being read", async () => {
    show("/rules?scope=builtin");
    fireEvent.click(await screen.findByRole("button", { name: "New rule" }));
    const dialog = screen.getByRole("dialog", { name: "New rule" });
    expect(within(dialog).getByRole("button", { name: "Scope" }).textContent).toContain("Server");
    fireEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
    await screen.findByTestId("editor-window");
    expect(screen.getByTestId("where").textContent).toBe("?scope=server");
  });
});
