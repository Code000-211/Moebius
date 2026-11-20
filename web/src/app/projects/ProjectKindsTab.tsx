/**
 * The Kinds tab of a project's settings: the kinds of issues of the organisation, ticked for the ones this project
 * offers; each tick is saved at once (`PUT /v1/projects/{id}/kinds`). The kinds themselves, with their fields, are
 * made on the Kinds page, which only an owner opens.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { listKinds, setProjectKinds } from "../../api/calls/kinds";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { useErrorToast } from "../../ui/lib/useErrorToast";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";

export function ProjectKindsTab({ project, canEdit }: { project: Project; canEdit: boolean }) {
  const queryClient = useQueryClient();
  const kinds = useQuery({ queryKey: keys.issueKinds, queryFn: listKinds });
  const offer = useMutation({
    mutationFn: (names: string[]) => setProjectKinds(project.id, names),
    onSuccess: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.projects }),
        queryClient.invalidateQueries({ queryKey: keys.issueKinds }),
      ]),
  });
  useErrorToast(offer.error, "Couldn't change the kinds.");
  if (kinds.isPending) return <SkeletonLines lines={3} message="Reading the kinds" />;
  if (kinds.isError)
    return (
      <ReadFailed
        error={kinds.error}
        fallback="The kinds could not be read. Try again."
        busy={kinds.isFetching}
        onRetry={() => void kinds.refetch()}
      />
    );
  const offered = (kind: { projects: number[] }) => kind.projects.includes(project.id);
  return (
    <div>
      <div className="flex flex-col gap-3">
        <p className="m-0 max-w-prose text-base text-secondary">
          The kinds of issues this project offers on its cards.{" "}
          {canEdit && (
            <>
              Kinds and their fields are made on the{" "}
              <Link className="underline" to="/kinds">
                Kinds page
              </Link>
              .
            </>
          )}
        </p>
        {kinds.data.length === 0 ? (
          <p className="m-0 text-base text-secondary">No kind has been made yet.</p>
        ) : (
          <fieldset className="m-0 border-0 border-t border-hairline p-0">
            <legend className="sr-only">Kinds this project offers</legend>
            {kinds.data.map((kind) => (
              <label
                key={kind.id}
                className="flex h-9 items-center gap-2 border-b border-hairline text-base"
              >
                <input
                  type="checkbox"
                  checked={offered(kind)}
                  disabled={!canEdit || offer.isPending}
                  onChange={(event) =>
                    offer.mutate(
                      kinds.data
                        .filter((each) =>
                          each.id === kind.id ? event.target.checked : offered(each),
                        )
                        .map((each) => each.name),
                    )
                  }
                />
                {kind.name}
              </label>
            ))}
          </fieldset>
        )}
      </div>
    </div>
  );
}
