/**
 * The ⋯ menu of a card, on the board and in the dock: Block… or Unblock, and Delete…. The menu only says what was
 * chosen; the page owns the questions that follow (`CardDialogs.tsx`) and the requests. It is a real menu — the
 * trigger is a button with a name, Enter and the arrow keys work, Escape closes it and returns to the button — and it
 * keeps every key and click that happens in it to itself, because the card it sits in opens on Enter, is picked up for
 * a drag on Space or a pointer move, and the dock closes on Escape, and the menu's layer is part of the card in the
 * tree that rendered it even though it draws over the page.
 *
 * ```tsx
 * <CardMenu card={card} onBlock={askToBlock} onUnblock={unblock} onDelete={askToDelete} />
 * ```
 */
import { DotsThreeIcon, PlayIcon, ProhibitIcon, TrashIcon } from "@phosphor-icons/react";
import { type SyntheticEvent, useRef } from "react";
import type { Card } from "../../api/types/board";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../../ui/components/DropdownMenu";
import { cn } from "../../ui/lib/cn";
import { isHeld } from "./model";

/** Keeps an event from reaching the card or the dock that the menu sits in. */
const keep = (event: SyntheticEvent) => event.stopPropagation();

export function CardMenu({
  card,
  onBlock,
  onUnblock,
  onDelete,
  className,
}: {
  card: Card;
  /** Asks for the reason to block with; the page opens its dialog. */
  onBlock: (card: Card) => void;
  onUnblock: (card: Card) => void;
  /** Asks before deleting; the page opens its dialog. */
  onDelete: (card: Card) => void;
  className?: string;
}) {
  // A choice that opens a dialog keeps the focus the dialog takes: the menu does not hand it back to its button.
  const opensDialog = useRef(false);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={`More actions for #${card.number}`}
        onPointerDown={keep}
        onKeyDown={keep}
        onClick={keep}
        className={cn(
          "inline-flex size-10 shrink-0 cursor-pointer items-center justify-center rounded-control bg-transparent text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text data-[state=open]:bg-hover data-[state=open]:text-text",
          className,
        )}
      >
        <DotsThreeIcon className="size-5" weight="bold" aria-hidden="true" />
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        onPointerDown={keep}
        onKeyDown={keep}
        onClick={keep}
        onCloseAutoFocus={(event) => {
          if (opensDialog.current) event.preventDefault();
          opensDialog.current = false;
        }}
      >
        {isHeld(card) ? (
          <DropdownMenuItem onSelect={() => onUnblock(card)}>
            <PlayIcon aria-hidden="true" />
            Unblock
          </DropdownMenuItem>
        ) : (
          <DropdownMenuItem
            onSelect={() => {
              opensDialog.current = true;
              onBlock(card);
            }}
          >
            <ProhibitIcon aria-hidden="true" />
            Block…
          </DropdownMenuItem>
        )}
        <DropdownMenuItem
          className="text-error-text"
          onSelect={() => {
            opensDialog.current = true;
            onDelete(card);
          }}
        >
          <TrashIcon aria-hidden="true" />
          Delete…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
