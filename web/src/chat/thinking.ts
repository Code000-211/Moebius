/**
 * Whether the thinking of an agent's turn is shown open, the person's own choice ("Show thinking expanded" on the
 * Appearance page). It is kept on the server with the person's other preferences, so it follows them to every browser;
 * it is read through the query cache and handed down by a context, so `Turn` takes it without a prop from each page
 * that draws one. Off unless the person turned it on, and off for a screen that has no provider.
 */
import { useQuery } from "@tanstack/react-query";
import { createContext, createElement, type ReactNode, useContext } from "react";
import { preferences } from "../api/calls/preferences";
import { keys } from "../api/keys";
import { usePreferenceSwitch } from "./preference";

const ThinkingContext = createContext(false);

/** Gives what is drawn inside it the person's choice; a test gives it a value, the app reads it from the server. */
export function ThinkingProvider({ value, children }: { value: boolean; children: ReactNode }) {
  return createElement(ThinkingContext.Provider, { value }, children);
}

/** The shell's provider: the choice as the server has it. */
export function ThinkingSetting({ children }: { children: ReactNode }) {
  const read = useQuery({ queryKey: keys.preferences, queryFn: preferences });
  return createElement(ThinkingProvider, {
    value: read.data?.show_thinking_expanded ?? false,
    children,
  });
}

/** Whether finished thinking is shown open. */
export function useThinkingExpanded(): boolean {
  return useContext(ThinkingContext);
}

/** The choice and the way to change it, for the page that has the switch. The switch moves at once and is put back if the save fails. */
export function useShowThinking(): { on: boolean; set: (on: boolean) => void } {
  return usePreferenceSwitch("show_thinking_expanded");
}
