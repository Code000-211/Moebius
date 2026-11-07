/**
 * The switch of a plugin: it turns the plugin off and on with the state it sends, shows the server's refusal in its own
 * words, and turns the last launcher off only after a dialog says runs would start unwrapped and the person confirms.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PluginSwitch } from "./PluginSwitch";
import { runsLabel } from "./PluginsPage";

afterEach(() => vi.unstubAllGlobals());

const UNWRAP = "this change leaves no launcher on, so runs would start unwrapped";

/** The switch of `box`, on the server that answers each PUT with `answer(body)`. */
function shown(enabled: boolean, answer: (body: Record<string, unknown>) => Response) {
  const puts: Array<{ url: string; body: Record<string, unknown> }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body ?? "{}"));
      puts.push({ url, body });
      return answer(body);
    }),
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <PluginSwitch name="box" enabled={enabled} />
    </QueryClientProvider>,
  );
  return puts;
}

const done = (enabled: boolean) =>
  Response.json({ name: "box", enabled, changed: true, unwrapped_runs: false, note: null });

describe("the switch of a plugin", () => {
  it("sends the other state and names the plugin in the route", async () => {
    const puts = shown(true, (body) => done(body.enabled === true));
    const toggle = screen.getByRole("switch", { name: "Plugin box" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(toggle);
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0]).toEqual({ url: "/v1/plugins/box/enabled", body: { enabled: false } });
  });

  it("turns a plugin that is off on", async () => {
    const puts = shown(false, () => done(true));
    expect(screen.getByRole("switch", { name: "Plugin box" }).getAttribute("aria-checked")).toBe(
      "false",
    );
    fireEvent.click(screen.getByRole("switch", { name: "Plugin box" }));
    await waitFor(() => expect(puts[0]?.body).toEqual({ enabled: true }));
  });

  it("says the refusal in the server's words", async () => {
    shown(
      true,
      () => new Response("box stays on because Coder would run unwrapped", { status: 422 }),
    );
    fireEvent.click(screen.getByRole("switch", { name: "Plugin box" }));
    await screen.findByText("box stays on because Coder would run unwrapped");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks before the last launcher goes, and sends the confirmation only when it is given", async () => {
    const puts = shown(true, (body) =>
      body.unwrap_runs === true ? done(false) : new Response(UNWRAP, { status: 409 }),
    );
    fireEvent.click(screen.getByRole("switch", { name: "Plugin box" }));
    const dialog = await screen.findByRole("dialog", { name: "Turn off the last launcher?" });
    expect(dialog.textContent).toContain("runs start unwrapped");
    expect(puts.map((put) => put.body)).toEqual([{ enabled: false }]);
    fireEvent.click(screen.getByRole("button", { name: "Keep it on" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(puts).toHaveLength(1);

    fireEvent.click(screen.getByRole("switch", { name: "Plugin box" }));
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Turn off, run unwrapped" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(puts.at(-1)?.body).toEqual({ enabled: false, unwrap_runs: true });
  });
});

describe("how a plugin is labelled", () => {
  it("says Off for a plugin that is off, whatever it was", () => {
    expect(runsLabel(null, false)).toEqual({ label: "Off", tone: "neutral" });
    expect(runsLabel(null)).toEqual({ label: "Built in", tone: "neutral" });
    const process = { program: "p", state: "running", restarts: 0, error: null, log: "l" } as const;
    expect(runsLabel(process, false).label).toBe("Off");
    expect(runsLabel(process).label).toBe("Running");
  });
});
