/**
 * The Runners entry of Settings: the machines that run harnesses over SSH, as the harness configuration repository names
 * them (`GET /v1/runners`), with how many runs each has now. Read only: they are changed in that repository.
 */
import { useQuery } from "@tanstack/react-query";
import { runners as readRunners } from "../../api/calls/harness";
import { keys } from "../../api/keys";
import { FieldRow, Rows } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { AreaColumn, AreaTitle } from "./parts";

export function RunnersPage() {
  const list = useQuery({ queryKey: keys.runners, queryFn: readRunners });
  return (
    <AreaColumn>
      <AreaTitle title="Runners" />
      {list.isError ? (
        <ReadFailed
          error={list.error}
          fallback="Could not read the runners."
          onRetry={() => void list.refetch()}
        />
      ) : list.isPending ? (
        <SkeletonList rows={2} />
      ) : list.data.length === 0 ? (
        <p className="m-0 text-base text-secondary">
          No runner. Runs happen on this machine; remote runners are named in the harness
          configuration repository.
        </p>
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
    </AreaColumn>
  );
}
