/**
 * The Skills page: the rows are a list a screen reader can count, a row opens its skill in the editor window, which
 * asks before a change is dropped, and the buttons that need a harness configuration repository say so where a keyboard
 * reaches them.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SkillsPage } from "./SkillsPage";

const DETAIL = {
  name: "house-style",
  description: "How this repository writes code.",
  text: "Use short sentences.",
  files: ["scripts/check.py"],
};

/** Shows the page with the skills the server lists; a skill's own address answers with `DETAIL`. */
function show(skills: unknown[]) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify(/\/v1\/skills\/house-style/.test(url) ? DETAIL : skills),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/skills"]}>
        <SkillsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("the list of skills", () => {
  it("is a list a screen reader can count, with no count line above it", async () => {
    show([
      { name: "moebius", description: "Operate Moebius.", built_in: true },
      { name: "house-style", description: "How this repository writes code." },
    ]);
    const list = await screen.findByRole("list");
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    expect(within(list).getByRole("button", { name: "Edit skill house-style" })).toBeTruthy();
    // A line that counted the rows would start the column above the card beside it, so the list does the counting.
    expect(screen.queryByText(/^\d+ skills?$/)).toBeNull();
  });
});

describe("editing a skill in the window", () => {
  const SKILLS = [{ name: "house-style", description: "How this repository writes code." }];

  it("opens the window on a row with the text and the files, and asks before dropping a change", async () => {
    show(SKILLS);
    expect(screen.queryByTestId("editor-window")).toBeNull();
    fireEvent.click(await screen.findByRole("button", { name: "Edit skill house-style" }));
    const window = await screen.findByTestId("editor-window");
    expect(
      (within(window).getByRole("textbox", { name: "SKILL.md" }) as HTMLTextAreaElement).value,
    ).toBe("Use short sentences.");
    expect(within(window).getByRole("button", { name: "scripts/check.py" })).toBeTruthy();
    // Nothing is typed yet, so Save waits.
    expect(within(window).getByRole("button", { name: "Save" }).hasAttribute("disabled")).toBe(
      true,
    );
    fireEvent.change(within(window).getByRole("textbox", { name: "SKILL.md" }), {
      target: { value: "Use shorter sentences." },
    });
    fireEvent.click(within(window).getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("alertdialog", { name: "Discard changes?" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
    expect(screen.getByTestId("editor-window")).toBeTruthy();
    fireEvent.click(within(window).getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    await waitFor(() => expect(screen.queryByTestId("editor-window")).toBeNull());
  });

  it("opens a file in the same window and goes back to the skill", async () => {
    show(SKILLS);
    fireEvent.click(await screen.findByRole("button", { name: "Edit skill house-style" }));
    fireEvent.click(await screen.findByRole("button", { name: "scripts/check.py" }));
    expect(
      await screen.findByRole("dialog", { name: "house-style · scripts/check.py" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("textbox", { name: "SKILL.md" })).toBeTruthy();
  });
});

describe("a server without a harness configuration repository", () => {
  it("says beside the buttons why they are quiet", async () => {
    show([]);
    await screen.findByTestId("harness-setup");
    expect(screen.getByRole("button", { name: "New skill" }).hasAttribute("disabled")).toBe(true);
    expect(
      screen.getByText("Needs a harness configuration repository: set it up below."),
    ).toBeTruthy();
  });
});
