// From BloopAI/vibe-kanban packages/ui/src/components/Input.tsx (Apache-2.0), changed to this app's design standard:
// the field is borderless on the sunken colour and 40px tall, the label and any error come from `ui/page/Field.tsx`,
// the line that explains the field is named by `aria-describedby` rather than read as part of its name, and the focus
// ring comes from the global `:focus-visible` rule.
import type * as React from "react";
import { cn } from "../lib/cn";
import { CONTROL, useFieldIds } from "../lib/field";
import { isImeConfirmation } from "../lib/imeComposition";

export interface InputProps extends React.ComponentProps<"input"> {
  onCommandEnter?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  onCommandShiftEnter?: (e: React.KeyboardEvent<HTMLInputElement>) => void;
  /** For a name, a path or an identifier, where one character more or less matters. */
  mono?: boolean;
}

function Input({
  className,
  type,
  onKeyDown,
  onCommandEnter,
  onCommandShiftEnter,
  mono,
  ...props
}: InputProps) {
  const named = useFieldIds({
    label: props["aria-label"],
    labelledBy: props["aria-labelledby"],
    describedBy: props["aria-describedby"],
  });
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
      aria-labelledby={named.labelledBy}
      aria-describedby={named.describedBy}
      className={cn(
        CONTROL,
        "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none",
        mono && "font-mono text-sm",
        className,
      )}
      {...props}
    />
  );
}

export { Input };
