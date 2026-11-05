/**
 * The settings plugins ask of a repository: drawn from what each plugin declares, saved whole for one plugin, a
 * refusal said with the server's sentence, and a word when no plugin asks anything. A plugin that brings its own page
 * is drawn by it, and by the fields when it cannot be.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PluginProjectSettings } from "../../api/types/projects";
import { loadPage, type Mount } from "../plugins/loadPage";
import { PluginSettings } from "./PluginSettings";

vi.mock("../plugins/loadPage", () => ({ loadPage: vi.fn() }));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.mocked(loadPage).mockReset();
});

const SANDBOX: PluginProjectSettings = {
  repository: 1,
  plugins: [
    {
      plugin: "podman-sandbox",
      version: "0.2.0",
      settings: [
        {
          key: "dockerfile",
          label: "Dockerfile",
          description: "The image of the runs.",
          input: "code",
          language: "dockerfile",
          value: "FROM debian\n",
        },
        { key: "network", label: "Network", description: "", input: "line", value: "" },
      ],
      ui: false,
    },
  ],
};

/** The same plugin with its own page. */
const DRAWN: PluginProjectSettings = {
  ...SANDBOX,
  plugins: SANDBOX.plugins.map((plugin) => ({ ...plugin, ui: true })),
};

/** A page with one button that sets the network, which says what it was handed. */
const page: Mount = (element, host) => {
  const said = document.createElement("p");
  said.textContent = `${host.scope} ${host.repository} ${host.values.dockerfile?.trim()}`;
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = "Open to the internet";
  button.onclick = () => host.change({ ...host.values, network: "internet" });
  element.append(said, button);
  return () => element.replaceChildren();
};

/** The panel, the server answering the read with `settings` and each save with `answer` (a status and a body). */
function shown(settings: PluginProjectSettings, answer: (body: unknown) => Response) {
  const puts: Array<{ url: string; body: unknown }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      if (init?.method === "PUT") {
        const body = JSON.parse(String(init.body));
        puts.push({ url, body });
        return answer(body);
      }
      return Response.json(settings);
    }),
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <PluginSettings repository={1} />
    </QueryClientProvider>,
  );
  return puts;
}

describe("the plugin settings of a repository", () => {
  it("draws each setting as the plugin declares it and saves the plugin's values whole", async () => {
    const puts = shown(SANDBOX, (body) => {
      const values = (body as { values: Record<string, string> }).values;
      const plugin = SANDBOX.plugins[0];
      return Response.json({
        ...plugin,
        settings: plugin?.settings.map((s) => ({ ...s, value: values[s.key] ?? "" })),
      });
    });
    await screen.findByRole("heading", { name: "podman-sandbox" });
    expect(screen.getByText("The image of the runs.")).toBeTruthy();
    expect(await screen.findByTestId("code-editor")).toBeTruthy();
    const save = screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    fireEvent.change(screen.getByLabelText("Network"), { target: { value: "internet" } });
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await screen.findByText("Saved. The plugin uses it from the next run.");
    expect(puts).toEqual([
      {
        url: "/v1/repositories/1/plugin-settings/podman-sandbox",
        body: { values: { dockerfile: "FROM debian\n", network: "internet" } },
      },
    ]);
    expect((screen.getByLabelText("Network") as HTMLInputElement).value).toBe("internet");
    expect(
      (screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("says why the server refused and keeps what was typed", async () => {
    shown(SANDBOX, () => new Response("Network is one line", { status: 422 }));
    await screen.findByRole("heading", { name: "podman-sandbox" });
    fireEvent.change(screen.getByLabelText("Network"), { target: { value: "moebius" } });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    const notice = await screen.findByText("Network is one line");
    expect(notice.closest("[role=alert]")?.textContent).toContain("Couldn't save.");
    expect((screen.getByLabelText("Network") as HTMLInputElement).value).toBe("moebius");
  });

  it("draws a plugin's own page and saves what it reports", async () => {
    vi.mocked(loadPage).mockResolvedValue(page);
    const puts = shown(DRAWN, (body) => {
      const values = (body as { values: Record<string, string> }).values;
      const plugin = DRAWN.plugins[0];
      return Response.json({
        ...plugin,
        settings: plugin?.settings.map((s) => ({ ...s, value: values[s.key] ?? "" })),
      });
    });
    await screen.findByText("project 1 FROM debian");
    expect(loadPage).toHaveBeenCalledWith("podman-sandbox", "0.2.0");
    expect(screen.queryByLabelText("Network")).toBeNull();
    const save = screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Open to the internet" }));
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await screen.findByText("Saved. The plugin uses it from the next run.");
    expect(puts).toEqual([
      {
        url: "/v1/repositories/1/plugin-settings/podman-sandbox",
        body: { values: { dockerfile: "FROM debian\n", network: "internet" } },
      },
    ]);
  });

  it("draws the page again from the saved values on a revert", async () => {
    vi.mocked(loadPage).mockResolvedValue(page);
    shown(DRAWN, () => new Response(null, { status: 500 }));
    fireEvent.click(await screen.findByRole("button", { name: "Open to the internet" }));
    fireEvent.click(screen.getByRole("button", { name: "Revert changes" }));
    await screen.findByText("project 1 FROM debian");
    expect(
      (screen.getByRole("button", { name: "Save settings" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(loadPage).toHaveBeenCalledTimes(2);
  });

  it("draws the fields and says why when the plugin's page cannot be drawn", async () => {
    vi.mocked(loadPage).mockRejectedValue(new Error("the page threw"));
    shown(DRAWN, () => new Response(null, { status: 500 }));
    await screen.findByText(/the page threw/);
    expect(screen.getByText(/Its settings are shown as fields/)).toBeTruthy();
    expect(screen.getByLabelText("Network")).toBeTruthy();
  });

  it("says so when no plugin asks anything of a repository", async () => {
    shown({ repository: 1, plugins: [] }, () => new Response(null, { status: 500 }));
    await screen.findByText(/No plugin on this server asks anything of a repository/);
  });
});
