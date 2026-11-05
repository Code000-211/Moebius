// @vitest-environment jsdom
/**
 * The ⋯ menu of a card: a named button that opens a menu with Block… or Unblock, and Delete…; the choice reaches the
 * page, and nothing that happens in the menu reaches the card or the dock it sits in — Enter would open the card,
 * Space would pick it up and Escape would close the dock.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Card } from "../../api/types/board";
import { CardMenu } from "./CardMenu";

afterEach(cleanup);

function card(extra: Partial<Card> = {}): Card {
  return {
    id: 7,
    repository_id: 1,
    number: 12,
    title: "Fix the parser",
    body: "",
    author: "person",
    column: "In progress",
    created_at: 0,
    note: null,
    retried_after: null,
    rounds: 0,
    tokens: 0,
    run: null,
    blocked_by: [],
    refusals: 0,
    change: null,
    parent: null,
    counts_as_sub_issue: true,
    closed: false,
    labels: [],
    claimed_by: null,
    ...extra,
  };
}

/** A menu inside a card that listens for the keys and clicks a card and a dock act on. */
function shown(of: Card) {
  const actions = {
    onBlock: vi.fn(),
    onUnblock: vi.fn(),
    onDelete: vi.fn(),
    onAddSubIssue: vi.fn(),
    onMoveParent: vi.fn(),
  };
  const reached = vi.fn();
  render(
    // biome-ignore lint/a11y/noStaticElementInteractions: stands for the card and the dock, which listen like this
    <div onKeyDown={reached} onClick={reached} onPointerDown={reached}>
      <CardMenu card={of} {...actions} />
    </div>,
  );
  const trigger = screen.getByRole("button", { name: "More actions for #12" });
  return { actions, reached, trigger };
}

/** Opens the menu the way a keyboard does. */
function open(trigger: HTMLElement) {
  trigger.focus();
  fireEvent.keyDown(trigger, { key: "Enter" });
}

describe("the menu of a card", () => {
  it("is a button with a name, and opens a menu with Block… and Delete…", () => {
    const { trigger } = shown(card());
    expect(trigger.getAttribute("aria-haspopup")).toBe("menu");
    open(trigger);
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Block…" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Delete…" })).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "Unblock" })).toBeNull();
  });

  it("asks to add a sub-issue below the card and to move it to another parent", () => {
    const { trigger, actions } = shown(card());
    open(trigger);
    fireEvent.click(screen.getByRole("menuitem", { name: "Add a sub-issue…" }));
    expect(actions.onAddSubIssue).toHaveBeenCalledWith(expect.objectContaining({ id: 7 }));
    open(trigger);
    fireEvent.click(screen.getByRole("menuitem", { name: "Move to another parent…" }));
    expect(actions.onMoveParent).toHaveBeenCalledWith(expect.objectContaining({ id: 7 }));
    expect(actions.onBlock).not.toHaveBeenCalled();
  });

  it("asks to block the card, and to delete it, and does nothing else", () => {
    const { trigger, actions } = shown(card());
    open(trigger);
    fireEvent.click(screen.getByRole("menuitem", { name: "Block…" }));
    expect(actions.onBlock).toHaveBeenCalledWith(expect.objectContaining({ id: 7 }));
    open(trigger);
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
    expect(actions.onDelete).toHaveBeenCalledWith(expect.objectContaining({ id: 7 }));
    expect(actions.onUnblock).not.toHaveBeenCalled();
  });

  it("offers Unblock, and not Block…, on a card that is held", () => {
    const { trigger, actions } = shown(card({ held: { reason: "later", by: "person", at: 1 } }));
    open(trigger);
    expect(screen.queryByRole("menuitem", { name: "Block…" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Delete…" })).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitem", { name: "Unblock" }));
    expect(actions.onUnblock).toHaveBeenCalledWith(expect.objectContaining({ id: 7 }));
    expect(actions.onBlock).not.toHaveBeenCalled();
  });

  it("keeps the pointer, clicks and keys of its button to itself", () => {
    const { trigger, reached } = shown(card());
    fireEvent.pointerDown(trigger);
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Enter" });
    fireEvent.keyDown(trigger, { key: " " });
    expect(reached).not.toHaveBeenCalled();
  });

  it("keeps the keys, clicks and pointer inside the menu to itself", () => {
    const { trigger, reached } = shown(card());
    open(trigger);
    const menu = screen.getByRole("menu");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    fireEvent.keyDown(menu, { key: " " });
    fireEvent.pointerDown(menu);
    fireEvent.click(screen.getByRole("menuitem", { name: "Block…" }));
    expect(reached).not.toHaveBeenCalled();
  });

  it("closes on Escape without closing what it sits in", () => {
    const { trigger, reached } = shown(card());
    open(trigger);
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(reached).not.toHaveBeenCalled();
  });
});
