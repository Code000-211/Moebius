/**
 * The Kinds tab of a project's settings: the kinds of issues of the organisation, ticked for the ones this project
 * offers; each tick is saved at once (`PUT /v1/projects/{id}/kinds`). The kinds themselves, with their fields, are
 * made on the Kinds page, which only an owner opens.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { listKinds, setProjectKinds } from "../../api/calls/kinds";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Box } from "../../ui/page/Box";
import { Notice } from "../../ui/page/Notice";
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
  if (kinds.isPending)
    return (
      <Box title="Kinds">
        <SkeletonLines lines={3} message="Reading the kinds" />
      </Box>
    );
  if (kinds.isError)
    return (
      <Box title="Kinds">
        <ReadFailed
          error={kinds.error}
          fallback="The kinds could not be read. Try again."
          busy={kinds.isFetching}
          onRetry={() => void kinds.refetch()}
        />
      </Box>
    );
  const offered = (kind: { projects: number[] }) => kind.projects.includes(project.id);
  return (
    <Box title="Kinds">
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
          <fieldset className="m-0 flex flex-col gap-1 border-0 p-0">
            <legend className="sr-only">Kinds this project offers</legend>
            {kinds.data.map((kind) => (
              <label key={kind.id} className="flex min-h-7 items-center gap-2 text-base">
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
        {offer.isError && (
          <Notice tone="error">
            <span className="font-semibold">Couldn't change the kinds. </span>
            {sayError(offer.error, "Try again.")}
          </Notice>
        )}
      </div>
    </Box>
  );
}
