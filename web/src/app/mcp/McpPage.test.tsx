/**
 * The draft of an MCP server: choosing another row keeps what was typed, and the editor that reopens on that draft
 * still says the work is waiting — the file has not changed, so there is something to save.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { McpPage } from "./McpPage";

const ROUTES: Record<string, unknown> = {
  "/v1/skills": [{ name: "moebius", description: "Operate Moebius.", built_in: true }],
  "/v1/mcp-servers": [
    { name: "docs", kind: "stdio" },
    { name: "search", kind: "stdio" },
  ],
  "/v1/mcp-servers/docs": {
    name: "docs",
    command: "docs-server",
    args: ["--root"],
    // biome-ignore lint/suspicious/noTemplateCurlyInString: a reference the server resolves from its environment
    env: { TOKEN: "${DOCS_TOKEN}" },
  },
  "/v1/mcp-servers/search": { name: "search", command: "search-server" },
};

function show() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify(ROUTES[path.split("?")[0] ?? path] ?? []),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/mcp-servers?server=docs"]}>
        <McpPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

/** Save, the one of the editor open now; being disabled is how it says there is nothing to write. */
const saveButton = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;

describe("the draft of a server", () => {
  it("survives another row being chosen, and still says it is unsaved", async () => {
    show();
    const command = await screen.findByLabelText(/Command/);
    fireEvent.change(command, { target: { value: "docs-server-2" } });
    expect(await screen.findByText("Unsaved changes")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /^search/ }));
    expect(await screen.findByDisplayValue("search-server")).toBeTruthy();
    // Nothing to write is said by the quiet button, not by a warning beside one that still works.
    expect(saveButton().disabled).toBe(true);
    expect(screen.queryByText("Nothing to save")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /^docs/ }));
    expect(await screen.findByDisplayValue("docs-server-2")).toBeTruthy();
    // The file still has `docs-server`, so the draft is work waiting for a save, not nothing to save.
    expect(await screen.findByText("Unsaved changes")).toBeTruthy();
    expect(saveButton().disabled).toBe(false);
  });
});

describe("a server without a harness configuration repository", () => {
  it("says beside the buttons why no server can be added", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, status: 200, text: async () => "[]" })),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={["/mcp-servers"]}>
          <McpPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    await screen.findByTestId("harness-setup");
    expect(screen.getByRole("button", { name: "New server" }).hasAttribute("disabled")).toBe(true);
    expect(
      screen.getByText("Needs a harness configuration repository: set it up below."),
    ).toBeTruthy();
  });
});
