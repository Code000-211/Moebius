import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { keys } from "../../api/keys";
import type { Person } from "../../api/types/server";
import { MentionTextArea } from "./MentionTextArea";

const person = (id: number, handle: string, name: string, disabled = false): Person => ({
  id,
  handle,
  name,
  role: "member",
  disabled,
  created_at: 0,
});

function Box() {
  const [value, setValue] = useState("");
  const client = new QueryClient();
  client.setQueryData(keys.people, [
    person(1, "lea", "Lea Kim"),
    person(2, "tom", "Tom"),
    person(3, "lee", "Lee", true),
  ]);
  return (
    <QueryClientProvider client={client}>
      <MentionTextArea aria-label="Comment" value={value} onValueChange={setValue} />
    </QueryClientProvider>
  );
}

afterEach(() => vi.restoreAllMocks());

function type(text: string) {
  const box = screen.getByRole("combobox", { name: "Comment" }) as HTMLTextAreaElement;
  fireEvent.change(box, { target: { value: text, selectionStart: text.length } });
  return box;
}

describe("a text area that completes @handles", () => {
  it("offers the people whose handle starts with what is typed, never a disabled one", async () => {
    render(<Box />);
    type("Hi @le");
    await screen.findByRole("option", { name: /@lea/ });
    expect(screen.queryByRole("option", { name: /@lee/ })).toBeNull();
    expect(screen.queryByRole("option", { name: /@tom/ })).toBeNull();
  });

  it("writes the chosen handle, with the keys or the mouse, and closes the list", async () => {
    render(<Box />);
    const box = type("Hi @");
    await screen.findAllByRole("option");
    fireEvent.keyDown(box, { key: "ArrowDown" });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(box.value).toBe("Hi @tom "));
    expect(screen.queryByRole("listbox")).toBeNull();

    type("Hi @le");
    fireEvent.click(await screen.findByRole("option", { name: /@lea/ }));
    await waitFor(() =>
      expect((screen.getByRole("combobox") as HTMLTextAreaElement).value).toBe("Hi @lea "),
    );
  });

  it("closes on Escape and offers nothing inside an address", async () => {
    render(<Box />);
    const box = type("Hi @le");
    await screen.findByRole("listbox");
    fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.queryByRole("listbox")).toBeNull();
    type("me@le");
    expect(screen.queryByRole("listbox")).toBeNull();
  });
});
