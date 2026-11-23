// @vitest-environment jsdom
/**
 * The editor of a page edits its body only: the front matter (the labels and any other key) is not in the text field
 * or the preview, and the save writes it back unchanged ahead of the edited text.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PageEditor } from "./PageEditor";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const FRONT = "---\ntitle: Deploy\nlabels: [Runbook, On call]\nowner: ada\n---\n";
const BODY = "# Deploy\n\nShip it.\n";

function edit(front: string, body: string) {
  const sent: Array<{ url: string; body: unknown }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: { method?: string; body?: string }) => {
      if (init?.method === "PUT") sent.push({ url, body: JSON.parse(init.body ?? "{}") });
      const reply = init?.method === "PUT" ? { path: "Deploy", name: "Deploy" } : [];
      return { ok: true, status: 200, text: async () => JSON.stringify(reply) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <RouterProvider
        router={createMemoryRouter(
          [
            {
              path: "*",
              element: (
                <PageEditor
                  path="Deploy"
                  folder=""
                  text={body}
                  front={front}
                  version="abc1234"
                  keys={[]}
                  onSaved={() => {}}
                  onCancel={() => {}}
                />
              ),
            },
          ],
          { initialEntries: ["/wiki?page=Deploy&view=edit"] },
        )}
      />
    </QueryClientProvider>,
  );
  return sent;
}

describe("the editor of a page with front matter", () => {
  it("shows and previews the body alone", () => {
    edit(FRONT, BODY);
    const field = screen.getByLabelText("Text") as HTMLTextAreaElement;
    expect(field.value).toBe(BODY);
    expect(screen.getByRole("region", { name: "Preview" }).textContent).not.toContain("labels");
    expect(screen.getByRole("region", { name: "Preview" }).textContent).not.toContain("owner");
  });

  it("saves an edit with the labels and the other keys as they were", async () => {
    const sent = edit(FRONT, BODY);
    fireEvent.change(screen.getByLabelText("Text"), {
      target: { value: "# Deploy\n\nShip it twice.\n" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]?.body).toMatchObject({
      text: `${FRONT}# Deploy\n\nShip it twice.\n`,
      base: "abc1234",
    });
  });

  it("saves a page without front matter as it is typed", async () => {
    const sent = edit("", BODY);
    fireEvent.change(screen.getByLabelText("Text"), { target: { value: "Changed\n" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]?.body).toMatchObject({ text: "Changed\n" });
  });
});
