// From BloopAI/vibe-kanban packages/ui/src/components/IconButton.tsx (Apache-2.0), changed to this app's design
// standard: 40px on a side, no border, `ghost` and `secondary` in place of the old default and tertiary, and a
// mandatory accessible name.

import type { Icon } from "@phosphor-icons/react";
import { cn } from "../lib/cn";

interface IconButtonProps {
  icon: Icon;
  iconClassName?: string;
  onClick?: () => void;
  disabled?: boolean;
  /** `ghost` sits in a toolbar; `secondary` is a filled square beside other controls. */
  variant?: "ghost" | "secondary";
  /** The name a screen reader reads; an icon-only control always needs one. */
  "aria-label": string;
  title?: string;
  className?: string;
}

export function IconButton({
  icon: IconComponent,
  iconClassName,
  onClick,
  disabled,
  variant = "ghost",
  "aria-label": ariaLabel,
  title,
  className,
}: IconButtonProps) {
  return (
    <button
      type="button"
      className={cn(
        "inline-flex size-10 shrink-0 cursor-pointer items-center justify-center rounded-control transition-colors duration-(--motion-routine) ease-out hover:text-text disabled:cursor-not-allowed disabled:opacity-45",
        variant === "ghost"
          ? "text-secondary hover:bg-hover"
          : "bg-sunken text-text hover:bg-hover",
        className,
      )}
      onClick={onClick}
      disabled={disabled}
      aria-label={ariaLabel}
      title={title}
    >
      <IconComponent className={cn("size-5", iconClassName)} weight="bold" />
    </button>
  );
}
