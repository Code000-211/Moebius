/**
 * The Projects page (the desktop app's `screens/repositories`): every repository Moebius has, with the board it opens,
 * its settings and files, and the way to remove it. Each repository is a card of its own and the cards are grouped by
 * space alone. Removing asks first, because it takes the repository's cards, changes and chats with it and deletes its
 * clone; the server refuses while one of its runs is still going, and its sentence is shown under the list.
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
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { boardPath } from "../pages";
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

  if (data.isPending) return <Loader message="Reading the projects" className="py-8" />;
  // A read that failed is not a server with no repository: the empty state below would say there is nothing to open.
  if (data.isError) {
    return (
      <>
        <PageHeader title="Projects" />
        <ReadFailed
          error={data.error}
          fallback="The projects could not be read. Try again."
          busy={data.isFetching}
          onRetry={() => void data.refetch()}
        />
      </>
    );
  }
  const boards = data.data?.boards ?? [];

  return (
    <>
      <PageHeader
        title="Projects"
        context="Every repository Moebius works in. Open one for its board, its settings and its files."
        actions={
          <Button asChild size="sm">
            <Link to="/projects/new">Add a repository</Link>
          </Button>
        }
      />
      {boards.length === 0 ? (
        <Empty
          title="No repository yet"
          body="Add a repository and Moebius clones it on the server. Its board opens as soon as the clone is there, and cards come from the issues you pick."
          action={
            <Button asChild size="sm">
              <Link to="/projects/new">Add a repository</Link>
            </Button>
          }
        />
      ) : (
        <ul className="m-0 flex list-none flex-col gap-3 p-0">
          {boards.map((board) => {
            const { repository } = board;
            const removing = remove.isPending && remove.variables === repository.id;
            return (
              <li
                key={repository.id}
                data-testid="repository"
                className="flex flex-wrap items-center gap-4 rounded-card bg-surface p-4 shadow-card"
              >
                <Link to={boardPath(repository.id)} className="min-w-0 flex-1 no-underline">
                  <span className="block break-words text-lg font-semibold text-text hover:underline">
                    {repository.name}
                  </span>
                  <span className="mt-1 block break-words text-sm text-secondary">
                    {`${forgeTag(repository)} · ${cardsText(board.cards.length)}`}
                  </span>
                </Link>
                {removing ? (
                  <span className="text-sm text-secondary" role="status">
                    Removing
                  </span>
                ) : asking === repository.id ? (
                  <>
                    <p className="m-0 min-w-0 flex-1 break-words text-base text-text">
                      {`Delete ${repository.name} with its cards and chats?`}
                    </p>
                    <Button size="sm" variant="secondary" onClick={() => setAsking(null)}>
                      Keep
                    </Button>
                    <Button size="sm" variant="danger" onClick={() => remove.mutate(repository.id)}>
                      Delete
                    </Button>
                  </>
                ) : (
                  <>
                    <Button asChild size="sm" variant="secondary">
                      <Link to={`/projects/${repository.id}`}>Settings and files</Link>
                    </Button>
                    <Button size="sm" variant="danger" onClick={() => setAsking(repository.id)}>
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
        <div className="mt-4">
          <Notice tone="error">
            <span className="font-semibold">Couldn't remove the repository. </span>
            {failed}
          </Notice>
        </div>
      )}
    </>
  );
}
