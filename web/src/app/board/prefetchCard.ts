/**
 * Starts the reads a card's page makes (its timeline, comments, runs, followers, cycle, milestones and the script of its column) before the
 * page that draws them is on screen. The dock and the properties are drawn from the snapshot, which is the biggest
 * answer of the server, so without this their own requests would wait for it; with it they run beside it, and the
 * queries find their answers when the dock mounts. A read the cache holds fresh is not asked again.
 */
import type { QueryClient } from "@tanstack/react-query";
import { cardRuns, comments, history, script } from "../../api/calls/board";
import { cardCycle } from "../../api/calls/cycles";
import { cardMilestones } from "../../api/calls/milestones";
import { followers } from "../../api/calls/notices";
import { keys } from "../../api/keys";

/** Reads what the card with `id` shows; `column` names its script when the card is known (the project and the column). */
export function prefetchCard(
  queryClient: QueryClient,
  id: number,
  where?: { project: number; column: string },
): void {
  void queryClient.prefetchQuery({ queryKey: keys.history(id), queryFn: () => history(id) });
  void queryClient.prefetchQuery({ queryKey: keys.comments(id), queryFn: () => comments(id) });
  void queryClient.prefetchQuery({ queryKey: keys.cardRuns(id), queryFn: () => cardRuns(id) });
  void queryClient.prefetchQuery({ queryKey: keys.followers(id), queryFn: () => followers(id) });
  void queryClient.prefetchQuery({ queryKey: keys.cardCycle(id), queryFn: () => cardCycle(id) });
  void queryClient.prefetchQuery({
    queryKey: keys.cardMilestones(id),
    queryFn: () => cardMilestones(id),
  });
  if (where) {
    void queryClient.prefetchQuery({
      queryKey: keys.script(where.project, where.column),
      queryFn: () => script(where.project, where.column),
    });
  }
}
