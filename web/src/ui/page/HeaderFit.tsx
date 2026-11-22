/**
 * What a one-row `PageHeader` tells what it holds about the room there is: the primary button's label is dropped
 * (`HeaderPrimary` reads `compactPrimary`) and the tabs that do not fit are moved into "More" (`Tabs` reads
 * `hiddenTabs` and says how many it has). The header decides from measured widths (`fitRules.ts`); outside a
 * one-row header there is no context and everything is drawn in full.
 */
import { createContext, type ReactNode, useContext } from "react";
import { Button, type ButtonProps } from "../components/Button";

export interface HeaderFit {
  /** The primary button shows only its icon. */
  compactPrimary: boolean;
  /** How many tabs, counted from the right, sit in "More". */
  hiddenTabs: number;
  /** The tabs report how many there are, so the header knows how far it may collapse. */
  setTabCount: (count: number) => void;
}

export const HeaderFitContext = createContext<HeaderFit | null>(null);

/**
 * The primary button of a one-row header that has an icon. When the header is short of room it keeps only the icon,
 * and the label stays as its `aria-label` and `title`, so it is still named and has a tooltip.
 */
export function HeaderPrimary({
  icon,
  label,
  ...props
}: Omit<ButtonProps, "children"> & { icon: ReactNode; label: string }) {
  const compact = useContext(HeaderFitContext)?.compactPrimary ?? false;
  return (
    <Button
      size="sm"
      {...props}
      aria-label={compact ? label : undefined}
      title={compact ? label : undefined}
    >
      {icon}
      {!compact && label}
    </Button>
  );
}
