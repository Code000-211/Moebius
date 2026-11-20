/**
 * What the tests of the package sheet share: a stubbed server (`stubServer`, `answers`, `requests`), the package
 * summaries and details it answers with, and `mount`, which renders the sheet with its provider and two openers.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";
import { SheetSlotProvider } from "../ui/page/SheetSlot";
import { PackageSheet } from "./PackageSheet";
import { PackageSheetProvider, usePackageSheet } from "./PackageSheetContext";

type Answer = { status?: number; body: unknown };
export const answers: Record<string, Answer | ((body: unknown) => Answer)> = {};
export const requests: Array<{ method: string; path: string; body: unknown }> = [];

/** Stubs `fetch` with `answers` for every test of the file that calls it, and cleans up after each. */
export function stubServer() {
  beforeEach(() => {
    requests.length = 0;
    for (const key of Object.keys(answers)) delete answers[key];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (path: string, init: RequestInit) => {
        const method = init.method ?? "GET";
        const body = init.body ? JSON.parse(String(init.body)) : null;
        requests.push({ method, path, body });
        const given = answers[`${method} ${path}`];
        if (!given) return new Response(`no stub for ${method} ${path}`, { status: 404 });
        const answer = typeof given === "function" ? given(body) : given;
        const text = typeof answer.body === "string" ? answer.body : JSON.stringify(answer.body);
        return new Response(text, { status: answer.status ?? 200 });
      }),
    );
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });
}

export const summary = (extra: Record<string, unknown> = {}) => ({
  name: "gate",
  kind: "plugin",
  version: "1.1.0",
  description: "Holds cards marked wip.",
  registry: "moebius-plugins",
  compatible: true,
  installed: null,
  ...extra,
});

export const info = (kind = "plugin", installed: unknown = null) => ({
  name: "gate",
  registry: "moebius-plugins",
  installed,
  versions: [
    {
      version: "1.1.0",
      kind,
      description: "Holds cards marked wip.",
      moebius: ">=0.1",
      compatible: true,
    },
    { version: "0.9.0", kind, description: "", moebius: ">=9.0", compatible: false },
  ],
});

export const snapshot = {
  boards: [],
  harness_changes: [],
  inbox: [],
};

function Where() {
  return <p data-testid="where">{useLocation().pathname + useLocation().search}</p>;
}

function Openers() {
  const sheet = usePackageSheet();
  return (
    <>
      <button type="button" onClick={() => sheet.open("moebius")}>
        open registries
      </button>
      <button type="button" onClick={() => sheet.open("pi", "Coder")}>
        open pi for Coder
      </button>
    </>
  );
}

export function mount() {
  answers["GET /v1/snapshot"] = { body: snapshot };
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={["/plugins"]}>
        <SheetSlotProvider>
          <PackageSheetProvider>
            <Openers />
            <PackageSheet />
            <Routes>
              <Route path="*" element={<Where />} />
            </Routes>
          </PackageSheetProvider>
        </SheetSlotProvider>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

export const sheet = () => screen.getByRole("dialog", { name: "Package sheet" });
export const inSheet = () => within(sheet());
export const openRegistries = () =>
  fireEvent.click(screen.getByRole("button", { name: "open registries" }));
