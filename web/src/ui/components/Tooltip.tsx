// From BloopAI/vibe-kanban packages/ui/src/components/Tooltip.tsx (Apache-2.0), changed to this app's design
// standard: the bubble is glass at the control radius with the raised shadow, at 12px, a tooltip supplements and never
// carries anything essential.

import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

interface TooltipProps {
  children: ReactNode;
  content: string;
  side?: "top" | "bottom" | "left" | "right";
  className?: string;
}

export function Tooltip({ children, content, side = "bottom", className }: TooltipProps) {
  return (
    <TooltipPrimitive.Provider delayDuration={300}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal>
          <TooltipPrimitive.Content
            side={side}
            sideOffset={4}
            className={cn(
              "glass z-tip flex items-center gap-2 rounded-control px-2 py-1 text-xs text-text shadow-raised",
              "animate-in fade-in-0 zoom-in-95 duration-(--motion-fast)",
              className,
            )}
          >
            <span>{content}</span>
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  );
}
