/**
 * The Projects page (the desktop app's `screens/repositories`): every repository Moebius has, with the board it opens,
 * its settings and files, and the way to remove it. Removing asks first, because it takes the repository's cards,
 * changes and chats with it and deletes its clone; the server refuses while one of its runs is still going, and its
 * sentence is shown under the list.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { removeRepository } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { Notice } from "../../ui/page/Notice";
import { boardPath } from "../pages";
import { Header } from "../shell/Page";
import { cardsText, forgeTag } from "./model";

export function ProjectsPage() {
  const queryClient = useQueryClient();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  /** The repository whose removal is being asked about. */
  const [asking, setAsking] = useState<number | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const remove = useMutation({
    mutationFn: removeRepository,
    onMutate: () => {
      setAsking(null);
      setFailed(null);
    },
    onError: (error) =>
      setFailed(isApiError(error) ? error.message : "The repository could not be removed."),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.start });
    },
  });

  if (data.isPending) return <Loader message="Reading the projects…" className="py-double" />;
  const boards = data.data?.boards ?? [];

  return (
    <>
      <div className="flex items-start justify-between gap-base">
        <Header title="Projects" />
        <Button asChild size="sm" variant="outline">
          <Link to="/projects/new">Add a repository</Link>
        </Button>
      </div>
      <p className="m-0 mb-[12px] text-[12.5px] text-normal">
        Every repository Moebius has; open one, or remove it with the work stored for it.
      </p>
      {boards.length === 0 ? (
        <p className="text-sm text-normal">
          There is no repository yet. Add one to give Moebius a place to work.
        </p>
      ) : (
        <ul className="m-0 flex list-none flex-col gap-base p-0">
          {boards.map((board) => {
            const { repository } = board;
            const removing = remove.isPending && remove.variables === repository.id;
            return (
              <li
                key={repository.id}
                data-testid="repository"
                className="flex flex-wrap items-center gap-base rounded-xl border bg-surface px-[14px] py-[10px]"
              >
                <Link
                  to={boardPath(repository.id)}
                  className="min-w-0 flex-1 text-inherit no-underline hover:text-accent"
                >
                  <span className="block truncate text-[14px] font-bold text-high">
                    {repository.name}
                  </span>
                  <span className="block truncate font-mono text-[12px] text-low">
                    {`${forgeTag(repository)} · ${cardsText(board.cards.length)}`}
                  </span>
                </Link>
                {removing ? (
                  <span className="text-[12.5px] text-low">Removing…</span>
                ) : asking === repository.id ? (
                  <>
                    <span className="text-[12.5px] text-low">
                      {`Delete ${repository.name} with its cards and chats?`}
                    </span>
                    <Button size="sm" variant="outline" onClick={() => setAsking(null)}>
                      Keep
                    </Button>
                    <Button
                      size="sm"
                      variant="destructive"
                      onClick={() => remove.mutate(repository.id)}
                    >
                      Delete
                    </Button>
                  </>
                ) : (
                  <>
                    <Button asChild size="sm" variant="outline">
                      <Link to={`/projects/${repository.id}`}>Settings and files</Link>
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => setAsking(repository.id)}>
                      Delete
                    </Button>
                  </>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {failed && (
        <div className="mt-base">
          <Notice tone="error">{failed}</Notice>
        </div>
      )}
    </>
  );
}
