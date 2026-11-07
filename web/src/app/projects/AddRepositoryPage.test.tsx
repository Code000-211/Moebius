/**
 * What "Add a repository" shows when the server refuses one: what happened and what to do next first, the command the
 * run printed kept behind it, and what was typed still in the fields.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AddRepositoryPage } from "./AddRepositoryPage";

afterEach(() => vi.unstubAllGlobals());

/** What the server answers for a private repository cloned over HTTPS: git, quoted whole. */
const PRIVATE_OVER_HTTPS =
  "git clone --quiet -- https://github.com/acme/private.git /srv/moebius/repos/private failed: " +
  "remote: Invalid username or token.\n" +
  "fatal: could not read Username for 'https://github.com': terminal prompts disabled";

/** The page, with the server refusing the next request with `message`. */
async function refused(message: string): Promise<Element> {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(message, { status: 400 })));
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <AddRepositoryPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  fireEvent.change(screen.getByLabelText(/^Name/), { target: { value: "private" } });
  fireEvent.change(screen.getByLabelText(/^URL or path/), {
    target: { value: "https://github.com/acme/private.git" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  return await screen.findByRole("alert");
}

/** The row a field and a refusal about it share: the field wrapper and what follows it. */
function rowOf(control: HTMLElement) {
  return control.closest("label")?.parentElement?.parentElement ?? null;
}

describe("a repository the server refused", () => {
  it("says what happened before the command that failed", async () => {
    const alert = await refused(PRIVATE_OVER_HTTPS);
    const first = alert.querySelector("p");
    expect(first?.textContent).toBe("The server cannot sign in to GitHub.");
    expect(alert.textContent).toContain(
      "Set the GitHub token under Settings and restart the server",
    );

    // The command the run printed is detail to open, not the sentence a person reads first.
    const detail = alert.querySelector("details");
    expect(detail?.textContent).toContain("git clone --quiet --");
    expect(detail?.querySelector("summary")?.textContent).toBe("What the server said");
    expect(detail?.hasAttribute("open")).toBe(false);
  });

  it("keeps what was typed and puts the refusal under the address", async () => {
    const alert = await refused(PRIVATE_OVER_HTTPS);
    const name = screen.getByLabelText(/^Name/) as HTMLInputElement;
    const url = screen.getByLabelText(/^URL or path/) as HTMLInputElement;
    expect(name.value).toBe("private");
    expect(url.value).toBe("https://github.com/acme/private.git");

    // The refusal is about the address, so it stands with that field and not with the name.
    expect(rowOf(url)?.contains(alert)).toBe(true);
    expect(rowOf(name)?.contains(alert)).toBe(false);
  });

  it("passes a sentence that needs no translating", async () => {
    const alert = await refused("a repository named private exists already");
    expect(alert.textContent).toBe("a repository named private exists already");
    expect(alert.querySelector("details")).toBeNull();
    // It is about the name, so it stands under the name field.
    expect(rowOf(screen.getByLabelText(/^Name/) as HTMLInputElement)?.contains(alert)).toBe(true);
  });

  it("says when the request never reached the server", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <AddRepositoryPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    fireEvent.change(screen.getByLabelText(/^Name/), { target: { value: "private" } });
    fireEvent.change(screen.getByLabelText(/^URL or path/), {
      target: { value: "https://github.com/acme/private.git" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe("The server could not be reached. Try again.");
  });
});
