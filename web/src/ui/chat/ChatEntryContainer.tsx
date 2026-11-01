// From BloopAI/vibe-kanban packages/ui/src/components/ChatEntryContainer.tsx (Apache-2.0), changed: only the `user` and `system` variants (Moebius has no plans to approve); the header is a real button when it can be toggled; the colours are Deep Velvet names.
import { CaretDownIcon, GearIcon, type Icon, UserIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

type Variant = "user" | "system";

interface VariantConfig {
  icon: Icon;
  border: string;
  headerBg: string;
  bg: string;
}

const variantConfig: Record<Variant, VariantConfig> = {
  user: { icon: UserIcon, border: "border-border", headerBg: "", bg: "" },
  system: { icon: GearIcon, border: "border-border", headerBg: "bg-card", bg: "" },
};

interface ChatEntryContainerProps {
  variant: Variant;
  title?: ReactNode;
  headerRight?: ReactNode;
  expanded?: boolean;
  onToggle?: () => void;
  children?: ReactNode;
  className?: string;
  isGreyed?: boolean;
}

export function ChatEntryContainer({
  variant,
  title,
  headerRight,
  expanded = false,
  onToggle,
  children,
  className,
  isGreyed,
}: ChatEntryContainerProps) {
  const config = variantConfig[variant];
  const HeaderIcon = config.icon;
  const header = (
    <>
      <HeaderIcon className="size-icon-xs shrink-0 text-low" />
      {title && <span className="flex-1 truncate text-left text-sm text-normal">{title}</span>}
      {onToggle && (
        <CaretDownIcon
          className={cn(
            "size-icon-xs shrink-0 text-low transition-transform",
            !expanded && "-rotate-90",
          )}
        />
      )}
    </>
  );

  return (
    <div
      className={cn(
        "w-full rounded-sm",
        config.border && "border",
        config.border,
        config.bg,
        isGreyed && "pointer-events-none opacity-50",
        className,
      )}
    >
      <div className={cn("flex items-center gap-base overflow-hidden rounded-sm", config.headerBg)}>
        {onToggle ? (
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center gap-base px-double py-base"
            onClick={onToggle}
            aria-expanded={expanded}
          >
            {header}
          </button>
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-base px-double py-base">
            {header}
          </div>
        )}
        {headerRight && <div className="pr-base">{headerRight}</div>}
      </div>
      {expanded && children && <div className="p-double">{children}</div>}
    </div>
  );
}
