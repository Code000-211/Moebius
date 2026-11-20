/**
 * Whether the thinking of an agent's turn is shown open, the person's own choice ("Show thinking expanded" on the
 * Appearance page). It is kept on the server with the person's other preferences, so it follows them to every browser;
 * it is read through the query cache and handed down by a context, so `Turn` takes it without a prop from each page
 * that draws one. Off unless the person turned it on, and off for a screen that has no provider.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createContext, createElement, type ReactNode, useContext } from "react";
import { preferences, savePreferences } from "../api/calls/preferences";
import { sayError } from "../api/errorText";
import { keys } from "../api/keys";
import { useToast } from "../ui/page/Toast";

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
  const client = useQueryClient();
  const toast = useToast();
  const read = useQuery({ queryKey: keys.preferences, queryFn: preferences });
  const save = useMutation({
    mutationFn: savePreferences,
    onMutate: async (next) => {
      await client.cancelQueries({ queryKey: keys.preferences });
      const before = client.getQueryData(keys.preferences);
      client.setQueryData(keys.preferences, next);
      return { before };
    },
    onError: (error, _next, context) => {
      client.setQueryData(keys.preferences, context?.before);
      toast({ kind: "error", text: sayError(error, "Could not save the setting") });
    },
    onSuccess: (kept) => client.setQueryData(keys.preferences, kept),
  });
  return {
    on: read.data?.show_thinking_expanded ?? false,
    set: (on) => save.mutate({ show_thinking_expanded: on }),
  };
}
