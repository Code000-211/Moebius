/**
 * The Export entry of Settings: the training-set exports the server has written (`GET /v1/exports`), newest first, with
 * how many judgements each holds. Read only: an export is started on the Decisions page or with `moebius export`
 * (documented in `docs/reference/training-export.md`).
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { trainingExports } from "../../api/calls/improve";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { ago, plural } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { FieldRow, Rows } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { AreaColumn, AreaTitle } from "./parts";

export function ExportPage() {
  const now = useNow();
  const list = useQuery({ queryKey: keys.exports, queryFn: trainingExports });
  return (
    <AreaColumn>
      <AreaTitle
        title="Export"
        actions={
          <Button asChild size="sm" variant="secondary">
            <Link to="/decision-model">Open Decisions</Link>
          </Button>
        }
      />
      {list.isError ? (
        <ReadFailed
          error={list.error}
          fallback="Could not read the exports."
          onRetry={() => void list.refetch()}
        />
      ) : list.isPending ? (
        <SkeletonList rows={2} />
      ) : list.data.length === 0 ? (
        <p className="m-0 text-base text-secondary">
          No export yet. Start one on the Decisions page, or run <code>moebius export</code>.
        </p>
      ) : (
        <Rows>
          {list.data.map((each) => {
            const total = each.kinds.reduce((sum, kind) => sum + kind.count, 0);
            return (
              <FieldRow
                key={each.id}
                label={`Export ${each.id}`}
                description={plural(each.kinds.length, "decision type", "decision types")}
              >
                <span className="text-sm text-secondary tabular-nums">
                  {plural(total, "judgement", "judgements")} · {ago(now, each.at)}
                </span>
              </FieldRow>
            );
          })}
        </Rows>
      )}
    </AreaColumn>
  );
}
