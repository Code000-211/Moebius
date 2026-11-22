/**
 * Whether code blocks and long lines wrap, the person's own choice ("Wrap long lines" on the Appearance page). Off, a
 * line longer than its block scrolls sideways; on, it breaks at the block's edge. It is kept on the server with the
 * person's other preferences and handed down by a context, like the thinking switch, so a code block, a diff and a
 * thread take it without a prop from each page that draws one. Off for a screen that has no provider.
 */
import { useQuery } from "@tanstack/react-query";
import { createContext, createElement, type ReactNode, useContext } from "react";
import { preferences } from "../api/calls/preferences";
import { keys } from "../api/keys";
import { usePreferenceSwitch } from "./preference";

const WrapContext = createContext(false);

/** Gives what is drawn inside it the person's choice; a test gives it a value, the app reads it from the server. */
export function WrapProvider({ value, children }: { value: boolean; children: ReactNode }) {
  return createElement(WrapContext.Provider, { value }, children);
}

/** The shell's provider: the choice as the server has it. */
export function WrapSetting({ children }: { children: ReactNode }) {
  const read = useQuery({ queryKey: keys.preferences, queryFn: preferences });
  return createElement(WrapProvider, { value: read.data?.wrap_long_lines ?? false, children });
}

/** Whether long lines wrap now. */
export function useWrapLines(): boolean {
  return useContext(WrapContext);
}

/** The choice and the way to change it, for the page that has the switch. */
export function useWrapSwitch(): { on: boolean; set: (on: boolean) => void } {
  return usePreferenceSwitch("wrap_long_lines");
}
