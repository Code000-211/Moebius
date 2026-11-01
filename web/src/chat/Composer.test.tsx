import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Composer } from "./Composer";

function Harness({ onSend, canSend = true }: { onSend: () => void; canSend?: boolean }) {
  const [value, setValue] = useState("hello");
  return <Composer value={value} onChange={setValue} onSend={onSend} canSend={canSend} />;
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
});
