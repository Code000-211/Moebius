// From BloopAI/vibe-kanban packages/ui/src/components/ChatEntryContainer.tsx (Apache-2.0), changed: only the `user`
// and `system` variants (Moebius has no plans to approve); the header is a real button when it can be toggled and is
// 40px tall; the container is a band of the sunken colour rather than a bordered box, so a message written by a person
// reads differently from an answer at a glance, without a coloured dot.
import { CaretDownIcon, GearIcon, type Icon, UserIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

type Variant = "user" | "system";

const ICONS: Record<Variant, Icon> = { user: UserIcon, system: GearIcon };

interface ChatEntryContainerProps {
  variant: Variant;
  /** Who wrote it, in words: "You", "You steered the chat". */
  title?: ReactNode;
  /** The controls at the right of the head row, such as Copy. */
  headerRight?: ReactNode;
  expanded?: boolean;
  onToggle?: () => void;
  children?: ReactNode;
  className?: string;
  /** Out of the way, because it is behind another message: no pointer, faded. */
  isGreyed?: boolean;
}

/** The band a message of the person or of the server is written in: a head row that says who, and the text under it. */
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
  const HeaderIcon = ICONS[variant];
  const head = (
    <>
      <HeaderIcon aria-hidden="true" className="size-4 shrink-0 text-secondary" />
      {title && (
        <span className="min-w-0 flex-1 break-words text-left text-base font-semibold text-text">
          {title}
        </span>
      )}
      {onToggle && (
        <CaretDownIcon
          aria-hidden="true"
          className={cn(
            "size-4 shrink-0 text-secondary transition-transform duration-(--motion-routine) ease-out",
            !expanded && "-rotate-90",
          )}
        />
      )}
    </>
  );

  return (
    <div
      className={cn(
        "reveal-copy w-full rounded-control bg-sunken",
        isGreyed && "pointer-events-none opacity-50",
        className,
      )}
    >
      <div className="flex min-h-10 items-center gap-3 px-4">
        {onToggle ? (
          <button
            type="button"
            className="flex min-h-10 min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-control text-left transition-colors duration-(--motion-routine) ease-out"
            onClick={onToggle}
            aria-expanded={expanded}
          >
            {head}
          </button>
        ) : (
          <div className="flex min-h-10 min-w-0 flex-1 items-center gap-3">{head}</div>
        )}
        {headerRight}
      </div>
      {expanded && children && <div className="px-4 pb-4">{children}</div>}
    </div>
  );
}
