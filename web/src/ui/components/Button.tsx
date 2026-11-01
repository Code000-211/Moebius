// From BloopAI/vibe-kanban packages/ui/src/components/Button.tsx (Apache-2.0), changed: colour classes use the Deep Velvet names (accent for the primary button, surface for the others); cn now merges, so the twMerge call is gone; React 19 ref prop in place of forwardRef.
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";

import { cn } from "../lib/cn";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-base whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent/60 disabled:pointer-events-none disabled:opacity-50 disabled:cursor-not-allowed",
  {
    variants: {
      variant: {
        default: "bg-accent text-on-fill border border-accent hover:bg-accent/90",
        destructive: "border border-error text-error hover:bg-error/10",
        outline: "border border-line-strong bg-panel text-high hover:bg-card-hover",
        secondary: "bg-card text-high border border-line hover:bg-card-hover",
        ghost: "text-normal hover:text-high hover:bg-card",
        link: "text-accent hover:underline",
        icon: "bg-transparent rounded-sm text-low hover:text-high",
      },
      size: {
        default: "h-9 px-4 py-2",
        xs: "h-7 px-2 text-xs",
        sm: "h-8 px-3",
        lg: "h-11 px-8",
        icon: "h-9 w-9",
      },
    },
    compoundVariants: [{ variant: "icon", class: "p-0 h-4" }],
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ComponentProps<"button">,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

function Button({ className, variant, size, asChild = false, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  return <Comp className={cn(buttonVariants({ variant, size, className }))} {...props} />;
}

export { Button, buttonVariants };
