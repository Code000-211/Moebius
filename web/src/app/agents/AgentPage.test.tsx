/**
 * The page of one agent: the tabs Settings, Instructions and Extensions (and the one in the address), Save that rests
 * until something changed and writes with the usual PUT, the guard that asks before a change is left, Delete that asks
 * first, the instructions in a light editor, the extensions added from pi.dev and removed, and the page of a new agent.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AgentPage } from "./AgentPage";

const COMPLETE = { model: "fake-small", protocol: "openai", skills: [], extensions: [], mcp: [] };

const ROUTES: Record<string, unknown> = {
  "/v1/skills": [
    { name: "moebius", description: "Operate Moebius.", built_in: true },
    { name: "house-style", description: "How we write.", built_in: false },
  ],
  "/v1/mcp-servers": [{ name: "docs", kind: "http" }],
  "/v1/runners": [],
  "/v1/models/available": { models: [], errors: [], classes: [], decision_models: [] },
  "/v1/agents": [
    { name: "Coder", model: "fake-small", protocol: "openai", harness: "pi-bolt", built_in: false },
    { name: "Fast", model: "small", protocol: "openai", harness: "pi-bolt", built_in: false },
    {
      name: "Reviewer",
      model: "fake-small",
      protocol: "openai",
      harness: "pi-bolt",
      built_in: true,
    },
  ],
  "/v1/agents/Coder": {
    name: "Coder",
    settings: { ...COMPLETE, extensions: ["extensions/log.ts"] },
    instructions: "Keep changes tiny.\n".repeat(60),
  },
  "/v1/agents/Fast": {
    name: "Fast",
    settings: { ...COMPLETE, model: "small", mcp: ["docs"] },
    instructions: "Be brief.",
  },
  "/v1/agents/Reviewer": {
    name: "Reviewer",
    settings: COMPLETE,
    instructions: "Say what is wrong.",
    built_in: { instructions: "You review.", skills: [], engineering_skills: [] },
  },
  "/v1/extensions/packages": [
    {
      name: "pi-subagents",
      source: "npm:pi-subagents",
      version: "1.2.0",
      resolved: "",
      integrity: null,
      sha256: null,
      resources: { extensions: ["a.ts"], skills: ["s"], prompts: [], themes: [] },
      install_scripts: [],
      agents: ["Coder"],
      error: null,
    },
    {
      name: "pi-other",
      source: "npm:pi-other",
      version: "0.3.0",
      resolved: "",
      integrity: null,
      sha256: null,
      resources: { extensions: ["b.ts"], skills: [], prompts: [], themes: [] },
      install_scripts: [],
      agents: ["Fast"],
      error: null,
    },
  ],
  "/v1/extensions/search": [
    {
      name: "pi-subagents",
      version: "1.2.0",
      description: "Run subagents.",
      kinds: ["extension"],
      weekly_downloads: 4200,
      npm: null,
      homepage: null,
      repository: null,
      date: null,
    },
    {
      name: "pi-themes",
      version: "2.0.0",
      description: "Colours for Pi.",
      kinds: ["theme"],
      weekly_downloads: 150,
      npm: null,
      homepage: null,
      repository: null,
      date: null,
    },
  ],
};

/** The requests that were not reads, as `[method, path, body]`. */
let writes: Array<[string, string, unknown]> = [];
/** What a write answers, by `METHOD path`, where it is not the default. */
let replies: Record<string, unknown> = {};
let router: ReturnType<typeof createMemoryRouter>;

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname + location.search}</p>;
}

function show(start = "/agents/Coder", routes: Record<string, unknown> = ROUTES) {
  writes = [];
  replies = {};
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string, init?: { method?: string; body?: string }) => {
      const route = path.split("?")[0] ?? path;
      if (init?.method && init.method !== "GET") {
        const body = init.body ? JSON.parse(init.body) : undefined;
        writes.push([init.method, route, body]);
        const reply = replies[`${init.method} ${route}`];
        if (reply !== undefined) {
          return { ok: true, status: 200, text: async () => JSON.stringify(reply) };
        }
        return {
          ok: true,
          status: 200,
          text: async () =>
            JSON.stringify({
              name: "x",
              package: "pi-themes",
              action: "add",
              agents: [],
              change: 7,
              ...body,
            }),
        };
      }
      return { ok: true, status: 200, text: async () => JSON.stringify(routes[route] ?? []) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  router = createMemoryRouter(
    [
      { path: "/agents", element: <Where /> },
      {
        path: "/agents/new",
        element: (
          <>
            <AgentPage creating />
            <Where />
          </>
        ),
      },
      {
        path: "/agents/:name",
        element: (
          <>
            <AgentPage />
            <Where />
          </>
        ),
      },
    ],
    { initialEntries: [start] },
  );
  return render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const saveButton = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
const tab = (name: string) => screen.getByRole("tab", { name: new RegExp(`^${name}`) });
const model = async () => (await screen.findByLabelText("Model")) as HTMLInputElement;

describe("the tabs of an agent's page", () => {
  it("has a breadcrumb, the name, and the tabs Settings, Instructions and Extensions with its count", async () => {
    show();
    await model();
    expect(screen.getByRole("link", { name: /^Agents/ }).getAttribute("href")).toBe("/agents");
    expect(screen.getByRole("heading", { name: "Coder" })).toBeTruthy();
    // One package names the Coder and one extension file is in its file.
    await waitFor(() =>
      expect(screen.getAllByRole("tab").map((each) => each.textContent)).toEqual([
        "Settings",
        "Instructions",
        "Extensions2",
      ]),
    );
    expect(tab("Settings").getAttribute("aria-selected")).toBe("true");
  });

  it("shows the settings as rows of the agent's file, with no window", async () => {
    show();
    expect(((await model()) as HTMLInputElement).value).toBe("fake-small");
    for (const label of ["Protocol", "Harness", "Thinking", "Token budget per card"]) {
      expect(screen.getByLabelText(label)).toBeTruthy();
    }
    expect(screen.getByRole("button", { name: "house-style" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "docs" })).toBeTruthy();
    expect(screen.queryByTestId("editor-window")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("opens the tab the address names, and a tab is put in the address", async () => {
    show("/agents/Coder?tab=instructions");
    expect(await screen.findByRole("textbox", { name: "Instructions" })).toBeTruthy();
    expect(tab("Instructions").getAttribute("aria-selected")).toBe("true");
    fireEvent.click(tab("Extensions"));
    await screen.findByTestId("agent-extensions");
    expect(screen.getByTestId("where").textContent).toBe("/agents/Coder?tab=extensions");
  });

  it("keeps what was typed while a tab is changed", async () => {
    show();
    fireEvent.change(await model(), { target: { value: "qwen-local" } });
    fireEvent.click(tab("Instructions"));
    fireEvent.click(tab("Settings"));
    expect(((await model()) as HTMLInputElement).value).toBe("qwen-local");
    expect(saveButton().disabled).toBe(false);
  });
});

describe("the instructions", () => {
  it("are in a large editor of the light editor colours, mono 13px, with long lines wrapping", async () => {
    show("/agents/Coder?tab=instructions");
    const editor = (await screen.findByRole("textbox", {
      name: "Instructions",
    })) as HTMLTextAreaElement;
    expect(editor.value.startsWith("Keep changes tiny.")).toBe(true);
    expect(editor.className).toContain("bg-editor-bg");
    expect(editor.className).toContain("text-editor-text");
    expect(editor.className).toContain("font-mono");
    expect(editor.className).toContain("text-[13px]");
    expect(editor.className).toContain("flex-1");
    expect(editor.getAttribute("wrap")).toBe("soft");
    // The dark code block is for fenced code only.
    expect(editor.className).not.toContain("bg-code");
  });

  it("saves what was typed with the usual PUT", async () => {
    show("/agents/Fast?tab=instructions");
    fireEvent.change(await screen.findByRole("textbox", { name: "Instructions" }), {
      target: { value: "Be very brief." },
    });
    fireEvent.click(saveButton());
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[0]).toBe("PUT");
    expect(writes[0]?.[1]).toBe("/v1/agents/Fast");
    expect(writes[0]?.[2]).toMatchObject({ instructions: "Be very brief." });
  });

  it("show what a built-in agent brings, read only, in the same light colours", async () => {
    show("/agents/Reviewer?tab=instructions");
    const builtIn = await screen.findByTestId("built-in-instructions");
    expect(builtIn.textContent).toBe("You review.");
    expect(builtIn.className).toContain("bg-editor-bg");
    expect(builtIn.className).not.toContain("bg-code");
  });
});

describe("Save and the guard", () => {
  it("rests until something changed, and saving keeps the page and says it is saved", async () => {
    show();
    await model();
    expect(saveButton().disabled).toBe(true);
    fireEvent.change(await model(), { target: { value: "qwen-local" } });
    expect(saveButton().disabled).toBe(false);
    fireEvent.click(saveButton());
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[1]).toBe("/v1/agents/Coder");
    expect(writes[0]?.[2]).toMatchObject({ settings: { model: "qwen-local" } });
    await waitFor(() => expect(saveButton().disabled).toBe(true));
    expect(screen.getByTestId("where").textContent).toBe("/agents/Coder");
    // Saved, so leaving asks nothing.
    await act(() => router.navigate("/agents"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks before a change is left, and Keep editing stays with it as typed", async () => {
    show();
    fireEvent.change(await model(), { target: { value: "qwen-local" } });
    await act(() => router.navigate("/agents"));
    const asked = await screen.findByRole("dialog");
    expect(within(asked).getByText(/not saved yet/)).toBeTruthy();
    fireEvent.click(within(asked).getByRole("button", { name: "Keep editing" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByTestId("where").textContent).toBe("/agents/Coder");
    expect(((await model()) as HTMLInputElement).value).toBe("qwen-local");
  });

  it("leaves when the change is given up", async () => {
    show();
    fireEvent.change(await model(), { target: { value: "qwen-local" } });
    await act(() => router.navigate("/agents"));
    fireEvent.click(await screen.findByRole("button", { name: "Leave without saving" }));
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/agents"));
    expect(writes).toEqual([]);
  });

  it("does not ask when nothing changed", async () => {
    show();
    await model();
    await act(() => router.navigate("/agents"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByTestId("where").textContent).toBe("/agents");
  });

  it("says a refusal under the head, and a bad token budget before anything is sent", async () => {
    show();
    fireEvent.change(await screen.findByLabelText("Token budget per card"), {
      target: { value: "0" },
    });
    fireEvent.click(saveButton());
    expect((await screen.findByRole("alert")).textContent).toBe(
      "The token budget is a whole number of at least 1.",
    );
    expect(writes).toEqual([]);
  });
});

describe("Delete", () => {
  it("asks first, deletes, and goes back to the list without the guard", async () => {
    show();
    await model();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("Delete this agent?")).toBeTruthy();
    expect(writes).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Delete agent" }));
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toEqual(["DELETE", "/v1/agents/Coder", undefined]);
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/agents"));
  });
});

describe("the extensions", () => {
  it("lists the packages that name this agent with their version and kinds, and not another agent's", async () => {
    show("/agents/Coder?tab=extensions");
    const rows = await screen.findAllByTestId("installed-package");
    expect(rows).toHaveLength(1);
    expect(rows[0]?.textContent).toContain("pi-subagents");
    expect(rows[0]?.textContent).toContain("1.2.0");
    expect(rows[0]?.textContent).toContain("Extension");
    expect(rows[0]?.textContent).toContain("Skill");
    expect(screen.getByText("extensions/log.ts")).toBeTruthy();
  });

  it("removes a package as a proposal and links to the change in the Inbox", async () => {
    show("/agents/Coder?tab=extensions");
    fireEvent.click(await screen.findByRole("button", { name: "Remove pi-subagents" }));
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[0]).toBe("DELETE");
    expect(writes[0]?.[1]).toBe("/v1/extensions/packages/pi-subagents");
    expect(await screen.findByTestId("package-report")).toBeTruthy();
    expect(screen.getByRole("link", { name: "See it in the Inbox" })).toBeTruthy();
  });

  it("offers the update the nightly check found, with its changelog, as a proposal", async () => {
    show("/agents/Coder?tab=extensions", {
      ...ROUTES,
      "/v1/snapshot": {
        boards: [],
        inbox: [
          {
            kind: "extension_update",
            data: { package: "pi-subagents", from: "1.2.0", to: "1.3.0" },
          },
        ],
      },
    });
    replies["POST /v1/extensions/packages/pi-subagents/update"] = {
      offered: [
        {
          package: "pi-subagents",
          action: "update",
          from: "1.2.0",
          to: "1.3.0",
          agents: [],
          change: 9,
          changelog: "Notes survive a restart.",
        },
      ],
      up_to_date: [],
      failed: [],
    };
    fireEvent.click(await screen.findByRole("button", { name: "Update pi-subagents" }));
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[1]).toBe("/v1/extensions/packages/pi-subagents/update");
    expect((await screen.findByTestId("package-report")).textContent).toContain(
      "Proposed updating pi-subagents from 1.2.0 to 1.3.0",
    );
    expect(screen.getByRole("region", { name: "Changelog of pi-subagents" }).textContent).toContain(
      "Notes survive a restart.",
    );
  });

  it("searches pi.dev with name, version, downloads, description and kind, and adds a package for the agent", async () => {
    show("/agents/Coder?tab=extensions");
    const results = await screen.findAllByTestId("package-result");
    expect(results).toHaveLength(2);
    expect(results[1]?.textContent).toContain("pi-themes");
    expect(results[1]?.textContent).toContain("2.0.0");
    expect(results[1]?.textContent).toContain("Colours for Pi.");
    expect(results[1]?.textContent).toContain("Theme");
    // The one the agent already has is not offered again.
    expect(within(results[0] as HTMLElement).queryByRole("button", { name: /^Add/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Add pi-themes" }));
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[0]).toBe("POST");
    expect(writes[0]?.[1]).toBe("/v1/extensions/packages");
    expect(writes[0]?.[2]).toMatchObject({ source: "npm:pi-themes", agent: "Coder" });
    expect(await screen.findByTestId("package-report")).toBeTruthy();
  });

  it("narrows the search by kind with the chips All, Extension, Skill, Theme and Prompt", async () => {
    show("/agents/Coder?tab=extensions");
    await screen.findAllByTestId("package-result");
    const kinds = within(screen.getByRole("group", { name: "Filter by kind" }));
    expect(kinds.getAllByRole("button").map((each) => each.textContent)).toEqual([
      "All",
      "Extension",
      "Skill",
      "Theme",
      "Prompt",
    ]);
    fireEvent.click(kinds.getByRole("button", { name: "Theme" }));
    await waitFor(() => expect(screen.getAllByTestId("package-result")).toHaveLength(1));
    expect(screen.getByTestId("package-result").textContent).toContain("pi-themes");
  });

  it("adds an extension file by its path, which Save writes", async () => {
    show("/agents/Fast?tab=extensions");
    fireEvent.change(await screen.findByLabelText("extension path"), {
      target: { value: "extensions/log.ts" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add extension" }));
    fireEvent.click(saveButton());
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[2]).toMatchObject({ settings: { extensions: ["extensions/log.ts"] } });
  });
});

describe("a new agent", () => {
  const routes: Record<string, unknown> = {
    ...ROUTES,
    "/v1/agents/Quick": { name: "Quick", settings: COMPLETE, instructions: "" },
  };

  it("starts blank, named in Settings, and is saved under that name and its page opens", async () => {
    show("/agents/new?blank=1", routes);
    expect(await screen.findByRole("heading", { name: "New agent" })).toBeTruthy();
    expect((await model()).value).toBe("");
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Quick" } });
    fireEvent.change(await model(), { target: { value: "qwen-local" } });
    fireEvent.click(saveButton());
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[1]).toBe("/v1/agents/Quick");
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/agents/Quick"));
  });

  it("copies an agent's settings and instructions under a new name, and refuses a name that is taken", async () => {
    show("/agents/new?from=Fast", routes);
    expect((await screen.findByDisplayValue("small")) as HTMLInputElement).toBeTruthy();
    const name = screen.getByLabelText("Name");
    fireEvent.change(name, { target: { value: "Fast" } });
    fireEvent.click(saveButton());
    expect(await screen.findByText(/An agent named Fast exists/)).toBeTruthy();
    expect(writes).toEqual([]);
    fireEvent.click(tab("Instructions"));
    expect(
      ((await screen.findByRole("textbox", { name: "Instructions" })) as HTMLTextAreaElement).value,
    ).toBe("Be brief.");
  });

  it("refuses the names that are addresses of Agents pages", async () => {
    show("/agents/new?blank=1", routes);
    fireEvent.change(await screen.findByLabelText("Name"), { target: { value: "skills" } });
    fireEvent.change(await model(), { target: { value: "m" } });
    fireEvent.click(saveButton());
    expect(await screen.findByText(/address of a page of Agents/)).toBeTruthy();
    expect(writes).toEqual([]);
  });

  it("cannot add a package before the agent exists", async () => {
    show("/agents/new?blank=1&tab=extensions", routes);
    await screen.findAllByTestId("package-result");
    expect(
      screen.getByRole("button", { name: "Add pi-themes" }).getAttribute("aria-disabled"),
    ).toBe("true");
  });
});
