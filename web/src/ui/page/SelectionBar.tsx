/**
 * The bar that floats at the foot of a page while rows or cards are chosen: how many, the commands for all of them,
 * and a button to let go. A command that has a key says it, and the key works while the bar is up (not in a field, a
 * menu or a dialog). Commands that share a `group` are one menu under the group's name, so a bar of three moves is one
 * button. Escape clears the choice (`useSelection`). The same commands are in the right-click menu (`ContextMenu`).
 *
 * ```tsx
 * <SelectionBar noun="card" count={3} commands={commands} onClear={choice.clear} />
 * ```
 */
import { CaretDownIcon, XIcon } from "@phosphor-icons/react";
import { useEffect, useRef } from "react";
import { Button } from "../components/Button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../components/DropdownMenu";
import { fieldHasKeys, modalIsOpen } from "../lib/shortcuts";

/** One command for what is chosen. */
export interface SelectionCommand {
  id: string;
  label: string;
  /** The single key that does it while the choice stands. */
  key?: string;
  /** The name of the menu it is under in the bar; the right-click menu lists it flat. */
  group?: string;
  /** Said in the bar's tooltip and the menu: why the command is not offered now. */
  disabled?: boolean;
  run: () => void;
}

/** Runs the command whose key was pressed, while `commands` stand; a key that a field or a dialog has is left alone. */
export function useSelectionKeys(commands: readonly SelectionCommand[]) {
  const latest = useRef(commands);
  latest.current = commands;
  const active = commands.length > 0;
  useEffect(() => {
    if (!active) return;
    function onKey(event: KeyboardEvent) {
      if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey) return;
      if (modalIsOpen() || fieldHasKeys(event.target)) return;
      const command = latest.current.find((c) => c.key === event.key.toLowerCase() && !c.disabled);
      if (!command) return;
      event.preventDefault();
      command.run();
    }
    // In the capture phase: while something is chosen, "a" assigns what is chosen and not the card in the dock.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [active]);
}

export function SelectionBar({
  noun,
  count,
  commands,
  onClear,
}: {
  /** What is chosen, in the singular: `card`, `run`. */
  noun: string;
  count: number;
  commands: readonly SelectionCommand[];
  onClear: () => void;
}) {
  useSelectionKeys(count > 0 ? commands : []);
  if (count === 0) return null;
  const groups = [...new Set(commands.flatMap((c) => (c.group ? [c.group] : [])))];
  return (
    <div
      role="toolbar"
      aria-label="Chosen"
      className="fixed bottom-20 left-1/2 z-sheet flex max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-1.5 rounded-card bg-panel py-1.5 pr-1.5 pl-4 shadow-raised"
    >
      <b role="status" className="whitespace-nowrap pr-1 text-base font-medium text-text">
        {count} {noun}
        {count === 1 ? "" : "s"} chosen
      </b>
      {commands
        .filter((c) => !c.group)
        .map((c) => (
          <Button
            key={c.id}
            variant="ghost"
            size="sm"
            disabled={c.disabled}
            onClick={c.run}
            aria-keyshortcuts={c.key}
          >
            {c.label}
          </Button>
        ))}
      {groups.map((group) => (
        <DropdownMenu key={group}>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm">
              {group}
              <CaretDownIcon className="size-3" weight="bold" aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="center">
            {commands
              .filter((c) => c.group === group)
              .map((c) => (
                <DropdownMenuItem
                  key={c.id}
                  disabled={c.disabled}
                  onSelect={c.run}
                  aria-keyshortcuts={c.key}
                >
                  <span className="flex-1">{c.label}</span>
                </DropdownMenuItem>
              ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ))}
      <Button variant="icon" size="sm" aria-label="Clear the choice (Escape)" onClick={onClear}>
        <XIcon className="size-4" aria-hidden="true" />
      </Button>
    </div>
  );
}
