/**
 * The way the page header brings back a sidebar that a narrow window hides (`narrow.ts`). The shell provides the
 * state; `PageHeader` draws the button while the window is narrow and the sidebar is closed. Outside the shell there
 * is no provider, and a header draws no button.
 */
import { createContext, useContext } from "react";

export interface SidebarToggle {
  /** The window is too narrow for the sidebar to stand beside the page. */
  narrow: boolean;
  /** The sidebar lies over the page now. */
  open: boolean;
  setOpen: (open: boolean) => void;
}

export const SidebarToggleContext = createContext<SidebarToggle | null>(null);

export function useSidebarToggle(): SidebarToggle | null {
  return useContext(SidebarToggleContext);
}
