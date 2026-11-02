// From BloopAI/vibe-kanban packages/ui/src/components/DropdownMenu.tsx (Apache-2.0), changed to this app's design
// standard: the menu is a surface at the card radius with the raised shadow and no border, every item is at least 40px
// tall with a 20px icon and a label that wraps, the hover is the hover fill rather than an accent, the chosen one of a
// radio group is set semibold on the selected fill rather than marked with a dot, and the layer comes from the tokens.
// React 19 ComponentRef in place of ElementRef.

import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import * as React from "react";

import { cn } from "../lib/cn";

/** The floating layer of a menu: a surface at the card radius, the raised shadow, no border. */
const SHEET =
  "z-menu min-w-48 rounded-card bg-surface p-2 text-text shadow-raised duration-(--motion-routine) " +
  "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 " +
  "data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 " +
  "data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 " +
  "data-[side=top]:slide-in-from-bottom-2 origin-(--radix-dropdown-menu-content-transform-origin)";

/** One choice: at least 40px tall, a 20px icon, the label wrapping rather than truncating. */
const ITEM =
  "flex min-h-10 cursor-pointer select-none items-center gap-3 rounded-control px-3 text-base text-text outline-none " +
  "transition-colors duration-(--motion-routine) ease-out focus:bg-hover focus:text-text " +
  "data-[disabled]:pointer-events-none data-[disabled]:opacity-45 " +
  "[&_svg]:pointer-events-none [&_svg]:size-5 [&_svg]:shrink-0";

const DropdownMenu = DropdownMenuPrimitive.Root;

const DropdownMenuTrigger = DropdownMenuPrimitive.Trigger;

const DropdownMenuRadioGroup = DropdownMenuPrimitive.RadioGroup;

const DropdownMenuContent = React.forwardRef<
  React.ComponentRef<typeof DropdownMenuPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Content>
>(({ className, sideOffset = 4, ...props }, ref) => (
  <DropdownMenuPrimitive.Portal>
    <DropdownMenuPrimitive.Content
      ref={ref}
      sideOffset={sideOffset}
      className={cn(
        SHEET,
        "max-h-(--radix-dropdown-menu-content-available-height) overflow-y-auto overflow-x-hidden",
        className,
      )}
      {...props}
    />
  </DropdownMenuPrimitive.Portal>
));
DropdownMenuContent.displayName = DropdownMenuPrimitive.Content.displayName;

const DropdownMenuItem = React.forwardRef<
  React.ComponentRef<typeof DropdownMenuPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Item> & {
    inset?: boolean;
  }
>(({ className, inset, ...props }, ref) => (
  <DropdownMenuPrimitive.Item
    ref={ref}
    className={cn(ITEM, "relative break-words", inset && "pl-10", className)}
    {...props}
  />
));
DropdownMenuItem.displayName = DropdownMenuPrimitive.Item.displayName;

const DropdownMenuRadioItem = React.forwardRef<
  React.ComponentRef<typeof DropdownMenuPrimitive.RadioItem>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.RadioItem>
>(({ className, children, ...props }, ref) => (
  <DropdownMenuPrimitive.RadioItem
    ref={ref}
    className={cn(
      // The chosen one is the chosen one because it reads as heavier and sits on the selected fill, not because a dot
      // is drawn beside it; `aria-checked` still carries it for a screen reader.
      ITEM,
      "break-words data-[state=checked]:bg-selected data-[state=checked]:font-semibold",
      className,
    )}
    {...props}
  >
    {children}
  </DropdownMenuPrimitive.RadioItem>
));
DropdownMenuRadioItem.displayName = DropdownMenuPrimitive.RadioItem.displayName;

const DropdownMenuLabel = React.forwardRef<
  React.ComponentRef<typeof DropdownMenuPrimitive.Label>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Label> & {
    inset?: boolean;
  }
>(({ className, inset, ...props }, ref) => (
  <DropdownMenuPrimitive.Label
    ref={ref}
    className={cn(
      "px-3 pt-2 pb-1 text-xs font-semibold text-secondary",
      inset && "pl-10",
      className,
    )}
    {...props}
  />
));
DropdownMenuLabel.displayName = DropdownMenuPrimitive.Label.displayName;

export {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
};
