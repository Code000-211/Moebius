/**
 * One switch of a person's preferences (`GET` and `PUT /v1/preferences`), for the page that has it: the Appearance
 * page. The switch moves at once and is put back, with a toast, if the save fails. The server replaces all the
 * preferences at once, so a save sends the others as they are in the query cache.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { preferences, savePreferences } from "../api/calls/preferences";
import { sayError } from "../api/errorText";
import { keys } from "../api/keys";
import type { Preferences } from "../api/types/preferences";
import { useToast } from "../ui/page/Toast";

/** A person who chose nothing has every switch off. */
const NONE: Preferences = { show_thinking_expanded: false, wrap_long_lines: false };

export function usePreferenceSwitch(key: keyof Preferences): {
  on: boolean;
  set: (on: boolean) => void;
} {
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
    on: read.data?.[key] ?? false,
    set: (on) => save.mutate({ ...NONE, ...read.data, [key]: on }),
  };
}
