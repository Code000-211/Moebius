// From BloopAI/vibe-kanban packages/ui/src/components/Button.tsx (Apache-2.0), changed to this app's design standard:
// the variants are primary, secondary, ghost, danger and attention; the primary fills with `--accent-fill`, edged in `--accent-border` with a white label; the attention one also wears an outline (not a ring, which is a shadow) in the
// warning's border colour, because its fill is close to the canvas of a warning banner; nothing else carries a border, a size under 28px, or a colour
// of its own; the focus ring comes from the
// global `:focus-visible` rule in `styles/tailwind.css`; a button that cannot be pressed and has a reason stays
// focusable (`aria-disabled`), shows the reason in the app's own tooltip on hover and on focus, and keeps it for a
// screen reader in `aria-describedby`, rather than in a line beside it or the browser's own tooltip box; Deep Velvet's
// coral accent is gone.
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { useId } from "react";

import { cn } from "../lib/cn";
import { Tooltip } from "./Tooltip";

/**
 * One implementation of a button, for every page.
 *
 * `primary` is the one clear action of a screen and fills with the indigo accent, a white label on it; `secondary` and `ghost` are quieter
 * alternatives beside it; `danger` is the destructive one, which says what it does in its label or, when it is only a
 * trashcan, in its `aria-label`, and `attention` is the action of a warning, which wears the warning's own colours.
 * Both hover to a token of their own (`--error-hover`, `--attention-hover`), opaque and checked for contrast in
 * `styles/tokens.test.ts`, so the text stays readable under the pointer. The sizes are the rows of the quiet
 * standard: 28px for `sm`, 32 for `md` and 36 for `lg`, at a radius of 6.
 */
const buttonVariants = cva(
  "inline-flex w-fit shrink-0 cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-control font-medium text-text transition-colors duration-(--motion-routine) ease-out disabled:cursor-not-allowed disabled:opacity-45 aria-disabled:cursor-not-allowed aria-disabled:opacity-45 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary:
          "bg-accent-fill text-on-accent outline outline-1 -outline-offset-1 outline-accent-border hover:bg-accent-fill/90",
        secondary:
          "bg-transparent text-text outline outline-1 -outline-offset-1 outline-hairline-strong hover:bg-hover",
        ghost: "bg-transparent text-secondary hover:bg-hover hover:text-text",
        danger: "bg-error-fill text-error-text hover:bg-error-hover",
        attention:
          "bg-attention-fill text-attention-text outline outline-1 -outline-offset-1 outline-attention-border hover:bg-attention-hover",
        icon: "bg-transparent text-secondary hover:bg-hover hover:text-text",
      },
      size: {
        sm: "min-h-7 px-2 text-base",
        md: "min-h-8 px-3 text-base",
        lg: "min-h-9 px-4 text-base",
        icon: "size-8",
      },
    },
    compoundVariants: [
      { variant: "icon", size: "sm", class: "size-7" },
      { variant: "icon", size: "md", class: "size-8" },
      { variant: "icon", size: "lg", class: "size-9" },
    ],
    defaultVariants: {
      variant: "primary",
      size: "md",
    },
  },
);

export interface ButtonProps
  extends React.ComponentProps<"button">,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

/**
 * A button that cannot be pressed and was given a `title` says why. It is not `disabled`, which would take it out of
 * the tab order and leave a mouse and a keyboard nothing to ask: it is `aria-disabled`, stays focusable, ignores
 * every click and Enter, and shows the reason in the app's own tooltip on hover and on focus. The browser's own
 * tooltip box for a `title` is a strip of another design system over the page, so the attribute is not passed on.
 * The words are also kept for a screen reader, in `aria-describedby`, and printed nowhere else: the reason beside a
 * control that cannot be clicked is the same words twice. A button with no reason is simply `disabled`.
 *
 * A button of none given is a plain one. Inside a `<form>` a `<button>` submits it whether or not that was meant,
 * which turns the Cancel of a dialog into the thing the dialog was asked not to do, so the default is the quiet one
 * and a form's own submit button says so.
 */
function Button({
  className,
  variant,
  size,
  asChild = false,
  disabled,
  title,
  type,
  onClick,
  "aria-describedby": describedBy,
  ...props
}: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  const classes = cn(buttonVariants({ variant, size, className }));
  const reason = useId();
  const withReason = !asChild && disabled === true && title !== undefined;
  // Anything else the caller pointed at is kept: a field's hint and the reason a button cannot be pressed are both
  // worth hearing, in the order a person meets them.
  const described = [describedBy, withReason ? reason : null].filter(Boolean).join(" ");
  const button = (
    <Comp
      className={classes}
      disabled={withReason ? undefined : disabled}
      aria-disabled={withReason ? true : undefined}
      title={withReason ? undefined : title}
      type={asChild ? type : (type ?? "button")}
      aria-describedby={described === "" ? undefined : described}
      onClick={
        withReason
          ? (event: React.MouseEvent<HTMLButtonElement>) => event.preventDefault()
          : onClick
      }
      {...props}
    />
  );
  if (!withReason) return button;
  return (
    <>
      <Tooltip content={title}>{button}</Tooltip>
      <span id={reason} className="sr-only">
        {title}
      </span>
    </>
  );
}

export { Button, buttonVariants };
