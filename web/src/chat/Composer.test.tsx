import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Composer } from "./Composer";

/**
 * The words a disabled button says to a screen reader and nobody sees, found the way a screen reader finds them:
 * through the button's own `aria-describedby`.
 */
function quietReason(button: HTMLElement): string | null {
  const id = button.getAttribute("aria-describedby");
  if (id === null) return null;
  const described = document.getElementById(id);
  expect(described?.classList.contains("sr-only")).toBe(true);
  return described?.textContent ?? null;
}

function Harness({
  onSend,
  canSend = true,
  sendHint,
}: {
  onSend: () => void;
  canSend?: boolean;
  sendHint?: string;
}) {
  const [value, setValue] = useState("hello");
  return (
    <Composer
      value={value}
      onChange={setValue}
      onSend={onSend}
      canSend={canSend}
      sendHint={sendHint}
    />
  );
}

describe("Composer", () => {
  it("sends with Enter", () => {
    const send = vi.fn();
    render(<Harness onSend={send} />);
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter" });
    expect(send).toHaveBeenCalledOnce();
  });

  it("makes a new line with Shift+Enter instead of sending", () => {
    const send = vi.fn();
    render(<Harness onSend={send} />);
    const proceeded = fireEvent.keyDown(screen.getByLabelText("Message"), {
      key: "Enter",
      shiftKey: true,
    });
    expect(send).not.toHaveBeenCalled();
    // The default action (the new line) is not prevented.
    expect(proceeded).toBe(true);
  });

  it("does not send when there is nothing to send, nor when Enter confirms an input method candidate", () => {
    const send = vi.fn();
    const { rerender } = render(<Harness onSend={send} canSend={false} />);
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter" });
    rerender(<Harness onSend={send} />);
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", isComposing: true });
    expect(send).not.toHaveBeenCalled();
  });

  it("sends with the Send button, which is off without something to send", () => {
    const send = vi.fn();
    const { rerender } = render(<Harness onSend={send} canSend={false} />);
    expect((screen.getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled).toBe(true);
    rerender(<Harness onSend={send} />);
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(send).toHaveBeenCalledOnce();
  });

  /**
   * An ended chat takes no message: what is sent starts a new chat that is told what was said in the one being read.
   * The button carries that word itself, since the sentence under the composer that used to explain it was asked for
   * and gone — a control that does something else has to say so where it is pressed.
   */
  it("says Continue, and what that starts, where the chat has ended", () => {
    const send = vi.fn();
    render(
      <Composer
        value="go on"
        onChange={() => {}}
        onSend={send}
        canSend
        sendLabel="Continue"
        sendTitle="Starts a new chat, told what was said in this one"
      />,
    );
    const button = screen.getByRole("button", { name: "Continue" }) as HTMLButtonElement;
    expect(button.getAttribute("title")).toBe("Starts a new chat, told what was said in this one");
    expect(screen.queryByRole("button", { name: "Send" })).toBeNull();
    fireEvent.click(button);
    expect(send).toHaveBeenCalledOnce();
  });

  /**
   * A disabled button is out of the tab order, so a `title` on it reaches the mouse and little else: screen readers do
   * not reliably read it and Firefox shows no tooltip on a disabled control at all. `Button` therefore keeps the words
   * in the button's own description and leaves the browser's box undrawn.
   */
  it("says nothing when Send merely has nothing to do, and the page's reason when it has one to give", () => {
    const send = vi.fn();
    const { rerender } = render(<Harness onSend={send} canSend={false} />);
    const plain = screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
    // An empty field is not a problem to explain: the button is disabled and quiet.
    expect(plain.getAttribute("title")).toBeNull();
    expect(quietReason(plain)).toBeNull();
    // The reason, when there is one, sits in the button's own description — never in a line beside a control that
    // cannot be pressed, and never in a box the browser draws in colours nobody chose.
    expect(plain.disabled).toBe(true);
    // A hint is kept for the case where the page knows something the person cannot — that a repository has to be
    // chosen first, say.
    rerender(<Harness onSend={send} canSend={false} sendHint="Choose a repository first" />);
    const hinted = screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
    expect(hinted.getAttribute("title")).toBeNull();
    // With a reason it stays focusable (`aria-disabled`), so a keyboard reaches the reason too.
    expect(hinted.getAttribute("aria-disabled")).toBe("true");
    expect(quietReason(hinted)).toBe("Choose a repository first");
    rerender(<Harness onSend={send} />);
    expect(
      screen.getByRole("button", { name: "Send" }).getAttribute("aria-describedby"),
    ).toBeNull();
  });

  /**
   * The app draws one focus ring, the global `:focus-visible` outline in `styles/tailwind.css`. A field that writes its
   * own `outline-none` cancels it and leaves the field a person uses most — on Chat, on Run, in the floating chat — with
   * no sign of where they are. The shared `TextArea` carries the control's classes and nothing over the ring.
   */
  it("is the shared control, and keeps the focus ring", () => {
    render(<Harness onSend={vi.fn()} />);
    const classes = screen.getByLabelText("Message").className.split(/\s+/);
    for (const ofTheControl of [
      "min-h-10",
      "w-full",
      "rounded-control",
      "bg-field",
      "field-edge",
      "px-3",
      "text-base",
      "text-text",
      "placeholder:text-muted",
    ]) {
      expect(classes, `missing ${ofTheControl}`).toContain(ofTheControl);
    }
    // The field still only grows: no drag handle, and a cap on how tall it gets.
    expect(classes).toContain("resize-none");
    expect(classes).toContain("max-h-60");
    // Nothing here writes over the outline the app gives a focused control.
    expect(classes.filter((name) => name.startsWith("outline-"))).toEqual([]);
  });
});
