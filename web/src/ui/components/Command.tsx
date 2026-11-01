// From BloopAI/vibe-kanban packages/ui/src/components/Command.tsx (Apache-2.0), changed to this app's design
// standard: every item is 40px tall and carries the control radius, the field of the query is the sunken fill rather
// than a rule, group headings are the small label rather than muted monospace, React 19 ComponentRef in place of
// ElementRef, and one word — Search — for the dialog, the sidebar's entry and the field of the query.

import { MagnifyingGlassIcon } from "@phosphor-icons/react";
import { Command as CommandPrimitive } from "cmdk";
import * as React from "react";
import { cn } from "../lib/cn";
import { Dialog, DialogContent, DialogTitle } from "./Dialog";

const Command = React.forwardRef<
  React.ComponentRef<typeof CommandPrimitive>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive>
>(({ className, ...props }, ref) => (
  <CommandPrimitive
    ref={ref}
    className={cn(
      "flex w-full min-h-0 flex-col gap-2 overflow-hidden bg-surface p-3 text-text",
      className,
    )}
    {...props}
  />
));
Command.displayName = CommandPrimitive.displayName;

interface CommandDialogProps extends React.ComponentProps<typeof Dialog> {
  onCloseAutoFocus?: (event: Event) => void;
  onOpenAutoFocus?: (event: Event) => void;
}

function CommandDialog({
  children,
  onCloseAutoFocus,
  onOpenAutoFocus,
  ...props
}: CommandDialogProps) {
  return (
    <Dialog {...props}>
      <DialogContent
        className="overflow-hidden p-0"
        hideCloseButton
        onCloseAutoFocus={onCloseAutoFocus}
        onOpenAutoFocus={onOpenAutoFocus}
        aria-describedby={undefined}
      >
        <DialogTitle className="sr-only">Search</DialogTitle>
        {children}
      </DialogContent>
    </Dialog>
  );
}

const CommandInput = React.forwardRef<
  React.ComponentRef<typeof CommandPrimitive.Input>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Input>
>(({ className, ...props }, ref) => (
  <div
    className="flex shrink-0 items-center gap-3 rounded-control bg-sunken px-4 py-2"
    cmdk-input-wrapper=""
  >
    <MagnifyingGlassIcon
      aria-hidden="true"
      className="size-5 shrink-0 text-secondary"
      weight="regular"
    />
    <CommandPrimitive.Input
      ref={ref}
      className={cn(
        "min-h-10 w-full bg-transparent text-base text-text outline-none",
        "placeholder:text-muted disabled:cursor-not-allowed disabled:opacity-45",
        className,
      )}
      {...props}
    />
  </div>
));
CommandInput.displayName = CommandPrimitive.Input.displayName;

const CommandList = React.forwardRef<
  React.ComponentRef<typeof CommandPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.List>
>(({ className, ...props }, ref) => (
  <CommandPrimitive.List
    ref={ref}
    className={cn("max-h-96 min-h-0 flex-1 overflow-y-auto overflow-x-hidden", className)}
    {...props}
  />
));
CommandList.displayName = CommandPrimitive.List.displayName;

const CommandEmpty = React.forwardRef<
  React.ComponentRef<typeof CommandPrimitive.Empty>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Empty>
>((props, ref) => (
  <CommandPrimitive.Empty
    ref={ref}
    className="px-4 py-8 text-center text-base text-secondary"
    {...props}
  />
));
CommandEmpty.displayName = CommandPrimitive.Empty.displayName;

const CommandGroup = React.forwardRef<
  React.ComponentRef<typeof CommandPrimitive.Group>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Group>
>(({ className, ...props }, ref) => (
  <CommandPrimitive.Group
    ref={ref}
    className={cn(
      "overflow-hidden pt-2 text-text first:pt-0",
      "[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pb-2",
      "[&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:text-secondary",
      className,
    )}
    {...props}
  />
));
CommandGroup.displayName = CommandPrimitive.Group.displayName;

const CommandSeparator = React.forwardRef<
  React.ComponentRef<typeof CommandPrimitive.Separator>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Separator>
>(({ className, ...props }, ref) => (
  <CommandPrimitive.Separator ref={ref} className={cn("h-px bg-hairline", className)} {...props} />
));
CommandSeparator.displayName = CommandPrimitive.Separator.displayName;

const CommandItem = React.forwardRef<
  React.ComponentRef<typeof CommandPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Item>
>(({ className, ...props }, ref) => (
  <CommandPrimitive.Item
    ref={ref}
    className={cn(
      "relative flex min-h-10 cursor-pointer select-none items-center gap-3 rounded-control px-3 text-base outline-none",
      "data-[selected=true]:bg-selected data-[selected=true]:text-text",
      "data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-45",
      className,
    )}
    {...props}
  />
));
CommandItem.displayName = CommandPrimitive.Item.displayName;

function CommandShortcut({ className, ...props }: React.HTMLAttributes<HTMLSpanElement>) {
  return <span className={cn("ml-auto text-sm text-secondary", className)} {...props} />;
}
CommandShortcut.displayName = "CommandShortcut";

export {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
};
