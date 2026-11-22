/**
 * The Runners tab of Fleet: the machines that run harnesses over SSH, as the harness configuration repository names
 * them (`GET /v1/runners`), with how many runs each has now. Read only: they are changed in that repository.
 */
import { useQuery } from "@tanstack/react-query";
import { runners as readRunners } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import { Empty } from "../../ui/page/Empty";
import { FieldRow, Rows } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";

export function RunnersPanel() {
  const list = useQuery({ queryKey: keys.runners, queryFn: readRunners });
  return (
    <div data-testid="runners-panel">
      {list.isError ? (
        <ReadFailed
          error={list.error}
          fallback="Could not read the runners."
          busy={list.isFetching}
          onRetry={() => void list.refetch()}
        />
      ) : list.isPending ? (
        <SkeletonList rows={2} />
      ) : list.data.length === 0 ? (
        <Empty
          title="No runner"
          body="Runs happen on this machine. Remote runners are named in the harness configuration repository."
        />
      ) : (
        <Rows>
          {list.data.map((each) => (
            <FieldRow
              key={each.name}
              label={each.name}
              description={each.user ? `${each.user}@${each.host}` : each.host}
            >
              <span className="text-sm text-secondary tabular-nums">
                {each.busy} of {each.max_runs} running
              </span>
            </FieldRow>
          ))}
        </Rows>
      )}
    </div>
  );
}
