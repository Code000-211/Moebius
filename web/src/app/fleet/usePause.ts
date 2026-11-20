/**
 * Pausing and resuming all agents, for the pages that offer it (Fleet's header, the palette). Whether the agents are
 * paused is the snapshot's `paused`, which the `runs_paused` and `runs_resumed` events keep current, so a pause made
 * from another client or the tray shows here too. A refusal comes back as `error`, in the server's words.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { snapshot } from "../../api/calls/board";
import { pauseRuns, resumeRuns } from "../../api/calls/runs";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";

export function usePause(enabled = true) {
  const queryClient = useQueryClient();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot(), enabled });
  const paused = data.data?.paused ?? null;
  const change = useMutation({
    mutationFn: () => (paused ? resumeRuns() : pauseRuns()),
    onSettled: () => queryClient.invalidateQueries({ queryKey: keys.snapshot }),
  });
  return {
    paused,
    busy: change.isPending,
    error: change.isError
      ? sayError(change.error, "Could not change the agents. Try again.")
      : null,
    /** Pauses all agents, or resumes them when they are paused. */
    toggle: () => change.mutate(),
  };
}
