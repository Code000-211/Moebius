// From BloopAI/vibe-kanban packages/ui/src/components/Input.tsx (Apache-2.0), changed: the border and focus colours are set here (the original left them to its caller); the file-input classes are gone; React 19 ref prop in place of forwardRef.
import type * as React from "react";
import { cn } from "../lib/cn";
import { isImeConfirmation } from "../lib/imeComposition";

export interface InputProps extends React.ComponentProps<"input"> {
  onCommandEnter?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  onCommandShiftEnter?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
}

function Input({
  className,
  type,
  onKeyDown,
  onCommandEnter,
  onCommandShiftEnter,
  ...props
}: InputProps) {
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.currentTarget.blur();
    }
    if (e.key === "Enter" && !isImeConfirmation(e.nativeEvent)) {
      if (e.metaKey && e.shiftKey) {
        onCommandShiftEnter?.(e);
      } else {
        onCommandEnter?.(e);
      }
    }
    onKeyDown?.(e);
  };

  return (
    <input
      type={type}
      onKeyDown={handleKeyDown}
      className={cn(
        "flex h-9 w-full rounded-md border border-line-strong bg-recessed px-3 py-2 text-sm text-high placeholder:text-low focus-visible:border-accent focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

export { Input };
