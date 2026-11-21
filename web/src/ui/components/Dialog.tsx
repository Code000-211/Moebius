// From BloopAI/vibe-kanban packages/ui/src/components/Dialog.tsx (Apache-2.0), changed to this app's design standard:
// a solid panel at the window radius (16) with the window shadow and no border, the overlay a dim scrim that does not blur the
// page (a window never blurs), the close control a 32px square, and a fade of 120ms at most. React 19 ComponentRef in place of ElementRef.
//
// The sheet is as tall as what is in it: `inset-0` with `margin: auto` centres a box whose height is set, so it is
// `h-fit` against a maximum, not a box stretched to the window with its content parked at the top.

import { XIcon } from "@phosphor-icons/react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as React from "react";
import { cn } from "../lib/cn";

/** The element that had the focus when the dialog opened, for the content to hand the focus back to. */
const Opener = React.createContext<React.RefObject<Element | null>>({ current: null });

/**
 * A dialog. Opened by state (`open`, with no trigger), it notes which element had the focus the moment it opens —
 * before the sheet's own content, an `autoFocus` field included, takes it — so that closing it with Escape gives the
 * focus back there rather than leaving it on the page's body.
 */
function Dialog({ open, ...props }: DialogPrimitive.DialogProps) {
  const opener = React.useRef<Element | null>(null);
  const was = React.useRef(false);
  if (open && !was.current && typeof document !== "undefined")
    opener.current = document.activeElement;
  was.current = open === true;
  return (
    <Opener.Provider value={opener}>
      <DialogPrimitive.Root open={open} {...props} />
    </Opener.Provider>
  );
}

function DialogPortal({ children, ...props }: DialogPrimitive.DialogPortalProps) {
  return <DialogPrimitive.Portal {...props}>{children}</DialogPrimitive.Portal>;
}

const DialogOverlay = React.forwardRef<
  React.ComponentRef<typeof DialogPrimitive.Overlay>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay
    ref={ref}
    className={cn(
      "fixed inset-0 z-scrim bg-scrim",
      "data-[state=open]:animate-in data-[state=closed]:animate-out",
      "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 duration-(--motion-fast)",
      className,
    )}
    {...props}
  />
));
DialogOverlay.displayName = DialogPrimitive.Overlay.displayName;

const DialogContent = React.forwardRef<
  React.ComponentRef<typeof DialogPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Content> & {
    hideCloseButton?: boolean;
    /** The command palette is the one window that is glass; every other window is a solid panel. */
    glass?: boolean;
  }
>(
  (
    {
      className,
      children,
      hideCloseButton = false,
      glass = false,
      onCloseAutoFocus,
      onOpenAutoFocus,
      ...props
    },
    ref,
  ) => {
    const opener = React.useContext(Opener);
    return (
      <DialogPortal>
        <DialogOverlay />
        <DialogPrimitive.Content
          ref={ref}
          // Radix hides the page behind the sheet but does not say so in the dialog's own role; `modalIsOpen` looks for it.
          aria-modal="true"
          onOpenAutoFocus={onOpenAutoFocus}
          onCloseAutoFocus={(event) => {
            onCloseAutoFocus?.(event);
            const back = opener.current;
            if (event.defaultPrevented || !(back instanceof HTMLElement) || !back.isConnected)
              return;
            if (back === document.body) return;
            event.preventDefault();
            back.focus();
          }}
          className={cn(
            "fixed inset-0 z-sheet m-auto flex h-fit max-h-[calc(100%_-_2rem)] w-full max-w-lg",
            "flex-col gap-4 overflow-y-auto p-6",
            "rounded-window shadow-window",
            glass ? "glass" : "bg-panel",
            "data-[state=open]:animate-in data-[state=closed]:animate-out",
            "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
            "duration-(--motion-fast)",
            className,
          )}
          {...props}
        >
          {children}
          {!hideCloseButton && (
            <DialogPrimitive.Close className="absolute top-2 right-2 inline-flex size-8 cursor-pointer items-center justify-center rounded-control text-secondary transition-colors duration-(--motion-routine) hover:bg-hover hover:text-text disabled:pointer-events-none">
              <XIcon className="size-4" />
              <span className="sr-only">Close</span>
            </DialogPrimitive.Close>
          )}
        </DialogPrimitive.Content>
      </DialogPortal>
    );
  },
);
DialogContent.displayName = DialogPrimitive.Content.displayName;

const DialogHeader = ({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div className={cn("flex flex-col gap-2 pr-8", className)} {...props} />
);
DialogHeader.displayName = "DialogHeader";

const DialogFooter = ({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn("flex flex-col-reverse gap-3 sm:flex-row sm:justify-end", className)}
    {...props}
  />
);
DialogFooter.displayName = "DialogFooter";

const DialogTitle = React.forwardRef<
  React.ComponentRef<typeof DialogPrimitive.Title>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Title
    ref={ref}
    className={cn("break-words text-lg font-semibold tracking-tight text-text", className)}
    {...props}
  />
));
DialogTitle.displayName = DialogPrimitive.Title.displayName;

const DialogDescription = React.forwardRef<
  React.ComponentRef<typeof DialogPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Description
    ref={ref}
    className={cn("break-words text-base text-secondary", className)}
    {...props}
  />
));
DialogDescription.displayName = DialogPrimitive.Description.displayName;

export { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle };
