// From BloopAI/vibe-kanban packages/ui/src/components/IconButton.tsx (Apache-2.0), changed: none beyond formatting (the colour names resolve to Deep Velvet in styles/tailwind.css).

import type { Icon } from "@phosphor-icons/react";
import { cn } from "../lib/cn";

interface IconButtonProps {
  icon: Icon;
  iconClassName?: string;
  onClick?: () => void;
  disabled?: boolean;
  variant?: "default" | "tertiary";
  "aria-label": string;
  title?: string;
  className?: string;
}

export function IconButton({
  icon: IconComponent,
  iconClassName,
  onClick,
  disabled,
  variant = "default",
  "aria-label": ariaLabel,
  title,
  className,
}: IconButtonProps) {
  const variantStyles = disabled
    ? "opacity-40 cursor-not-allowed"
    : variant === "default"
      ? "text-low hover:text-normal hover:bg-secondary/50"
      : "bg-panel hover:bg-secondary text-normal";

  return (
    <button
      type="button"
      className={cn(
        "flex items-center justify-center p-half rounded-sm transition-colors",
        variantStyles,
        className,
      )}
      onClick={onClick}
      disabled={disabled}
      aria-label={ariaLabel}
      title={title}
    >
      <IconComponent className={cn("size-icon-sm", iconClassName)} weight="bold" />
    </button>
  );
}
