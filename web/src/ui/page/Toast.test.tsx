// @vitest-environment jsdom
/**
 * A toast goes by itself after its delay, goes at once with its close icon, says an error as an alert and a
 * confirmation as a status, and offers Undo only when it was given one.
 */
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type ToastOptions, ToastProvider, useToast } from "./Toast";

function Caller({ options }: { options: ToastOptions }) {
  const toast = useToast();
  return (
    <button type="button" onClick={() => toast(options)}>
      show
    </button>
  );
}

function show(options: ToastOptions) {
  render(
    <ToastProvider>
      <Caller options={options} />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "show" }));
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("a toast", () => {
  it("is removed after its delay", () => {
    show({ kind: "confirmation", text: "Saved", delay: 1000 });
    expect(screen.getByText("Saved")).toBeTruthy();
    act(() => vi.advanceTimersByTime(999));
    expect(screen.queryByText("Saved")).toBeTruthy();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByText("Saved")).toBeNull();
  });

  it("is removed by its close icon", () => {
    show({ kind: "confirmation", text: "Saved" });
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("Saved")).toBeNull();
  });

  it("is an alert for an error and a status for a confirmation", () => {
    show({ kind: "error", text: "Broke" });
    expect(screen.getByRole("alert").textContent).toContain("Broke");
    cleanup();
    show({ kind: "confirmation", text: "Done" });
    expect(screen.getByRole("status").textContent).toContain("Done");
  });

  it("calls Undo once and disappears", () => {
    const undo = vi.fn();
    show({ kind: "confirmation", text: "Moved", undo });
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(undo).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Moved")).toBeNull();
    act(() => vi.advanceTimersByTime(60_000));
    expect(undo).toHaveBeenCalledTimes(1);
  });

  it("offers no Undo unless it was given one", () => {
    show({ kind: "confirmation", text: "Saved" });
    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
  });
});
