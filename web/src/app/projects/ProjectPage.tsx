/**
 * The page of one repository under its project (`/projects/:projectId/repositories/:repoId`; the project's settings are `ProjectSettingsPage`). A repository the project does not hold is said not to be there, and the address is left as it is: where it is and what it
 * trusts, with the logins of a repository on a forge to edit, the settings Moebius keeps for it, the settings plugins
 * ask of it, and its files on the default branch, which can be read, edited and saved as a commit. The parts are
 * borderless cards grouped by space. The board of the repository is its own page. Delete, in the header, removes the
 * repository and asks first, because it takes the repository's cards, changes and chats with it and deletes its clone;
 * the server refuses while one of its runs is still going, and its sentence is shown in the dialog. The repository that
 * holds a project's own files (`files_of`) is shown with the project's name and has no Delete: it goes only with its
 * project.
 */
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { editTrusted, listProjects, removeRepository } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Repository } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { Box } from "../../ui/page/Box";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { boardPath, settingsPath } from "../pages";
import { FilesPanel } from "./FilesPanel";
import { forgeTag, holdsRepository, logins, onForge } from "./model";
import { PluginSettings } from "./PluginSettings";
import { ReviewFlow } from "./ReviewFlow";
import { SettingsPanel } from "./SettingsPanel";
import { StaleHandling } from "./StaleHandling";
import { TestLane } from "./TestLane";
import { Templates } from "./Templates";
import { WorkingWithOthers } from "./WorkingWithOthers";

export function ProjectPage() {
  const { projectId, repoId } = useParams();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const failed = data.isError ? data : projects.isError ? projects : null;
  // A read that failed is not a repository that has gone: saying so comes before the page that says it is not here.
  if (failed) {
    return (
      <>
        <PageHeader title="Project" />
        <ReadFailed
          error={failed.error}
          fallback="The repository could not be read. Try again."
          busy={failed.isFetching}
          onRetry={() => void failed.refetch()}
        />
      </>
    );
  }
  if (!data.data || !projects.data)
    return (
      <>
        <PageHeader title="Project" />
        <SkeletonCard lines={4} message="Reading the repository" />
      </>
    );
  const project = projects.data.find((each) => String(each.id) === projectId);
  const board = data.data.boards.find((each) => String(each.repository.id) === repoId);
  if (!project || !board || !holdsRepository(project, board.repository.id)) {
    return (
      <>
        <PageHeader title="Project" />
        <Notice
          tone="error"
          action={
            <Button asChild size="sm" variant="secondary">
              <Link to={project ? boardPath(project.id) : "/projects"}>
                {project ? "Open the board" : "All projects"}
              </Link>
            </Button>
          }
        >
          This project has no repository with that address. It may have been removed.
        </Notice>
      </>
    );
  }
  return <RepositoryView repository={board.repository} projectId={project.id} />;
}

/** The page of `repository`, once the snapshot has it. */
function RepositoryView({ repository, projectId }: { repository: Repository; projectId: number }) {
  const filesOf = repository.files_of ?? null;
  const projects = useQuery({
    queryKey: keys.projects,
    queryFn: listProjects,
    enabled: filesOf !== null,
  });
  const project = projects.data?.find((each) => each.id === filesOf);
  return (
    <>
      <PageHeader
        title={project?.name ?? repository.name}
        back={
          <Tooltip content="All projects" side="bottom">
            <Button asChild variant="icon" aria-label="All projects">
              <Link to="/projects">
                <ArrowLeftIcon className="size-5" />
              </Link>
            </Button>
          </Tooltip>
        }
        actions={
          <>
            <Button asChild size="sm" variant="secondary">
              <Link to={settingsPath(projectId)}>Project settings</Link>
            </Button>
            <Button asChild size="sm" variant="secondary">
              <Link to={boardPath(projectId, repository.id)}>Open board</Link>
            </Button>
            {filesOf === null && <Remove repository={repository} />}
          </>
        }
      />
      <div className="flex flex-col gap-6">
        <Box title="Settings">
          <dl className="m-0 mb-4 flex flex-wrap gap-4">
            <Fact label="Forge" value={forgeTag(repository)} />
            <Fact label="Default branch" value={repository.default_branch} mono />
            <Fact label="Clone on the server" value={repository.path} mono />
          </dl>
          {filesOf !== null ? (
            <p className="m-0 max-w-prose text-base text-secondary">
              These are the project's own files, kept on the server without a forge. Its cards are
              worked on, reviewed and merged here until it has a repository.
            </p>
          ) : onForge(repository) ? (
            <Trusted key={repository.trusted.join(",")} repository={repository} />
          ) : (
            <p className="m-0 max-w-prose text-base text-secondary">
              This repository is on no forge, so it has no trusted authors: its issues come from
              here.
            </p>
          )}
        </Box>
        {filesOf === null && onForge(repository) && (
          <WorkingWithOthers
            key={`${repository.mark_ready}-${repository.ready_label}-${repository.agents_take}-${repository.commands}-${repository.command_limit}-${repository.answer_on_forge}-${repository.merge_reviewed}-${repository.require_measurement}-${repository.locked_paths?.join(",")}-${repository.review_brief?.join(",")}-${repository.testing_label}-${repository.measurers?.join(",")}-${repository.frozen?.since}`}
            repository={repository}
          />
        )}
        {filesOf === null && onForge(repository) && <ReviewFlow repository={repository.id} />}
        {filesOf === null && onForge(repository) && <TestLane repository={repository.id} />}
        {filesOf === null && onForge(repository) && <StaleHandling repository={repository.id} />}
        {filesOf === null && onForge(repository) && <Templates repository={repository.id} />}
        <PluginSettings repository={repository.id} />
        <FilesPanel repository={repository.id} />
        <SettingsPanel repository={repository.id} project={projectId} />
      </div>
    </>
  );
}

/** One fact of the repository: its label, and the value under it. A path or a branch is monospace and wraps. */
function Fact({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-48 flex-1 rounded-control bg-sunken p-3">
      <dt className="text-sm font-semibold text-secondary">{label}</dt>
      <dd
        className={cn("m-0 mt-1 break-all text-text", mono ? "font-mono text-sm" : "text-base")}
        title={value}
      >
        {value}
      </dd>
    </div>
  );
}

/** Deleting the repository, asked once; the page goes back to the projects when it is done. */
function Remove({ repository }: { repository: Repository }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button size="sm" variant="danger" onClick={() => setOpen(true)}>
        Delete repository
      </Button>
      <ConfirmDelete
        open={open}
        onOpenChange={setOpen}
        title={`Delete ${repository.name}?`}
        onConfirm={async () => {
          await removeRepository(repository.id);
          await Promise.all(
            [keys.snapshot, keys.start, keys.projects].map((queryKey) =>
              queryClient.invalidateQueries({ queryKey }),
            ),
          );
          navigate("/projects");
        }}
      >
        {`${repository.name} goes with its cards, changes and chats, and its clone on the server is deleted. The repository on its forge stays.`}
      </ConfirmDelete>
    </>
  );
}

/** The logins whose issues need no approval, as a line with the way to change them. */
function Trusted({ repository }: { repository: Repository }) {
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState<string | null>(null);
  const save = useMutation({
    mutationFn: (typed: string) => editTrusted(repository.id, logins(typed)),
    onSuccess: () => {
      setEditing(null);
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
    },
  });
  if (editing === null) {
    return (
      <div className="flex flex-wrap items-center gap-4">
        <p className="m-0 min-w-0 flex-1 break-words text-base text-text" data-testid="trusted">
          {repository.trusted.length === 0
            ? "No trusted authors"
            : `Trusts ${repository.trusted.join(", ")}`}
        </p>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => {
            save.reset();
            setEditing(repository.trusted.join(", "));
          }}
        >
          Edit trusted authors
        </Button>
      </div>
    );
  }
  return (
    <form
      className="flex max-w-md flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (!save.isPending) save.mutate(editing);
      }}
    >
      <Field
        label="Trusted authors"
        hint="Logins on the forge, comma separated. Their issues need no approval."
      >
        <Input
          value={editing}
          placeholder="leon, mona"
          autoComplete="off"
          onChange={(e) => setEditing(e.target.value)}
          autoFocus
        />
      </Field>
      <div className="flex gap-2">
        <Button type="submit" size="sm" disabled={save.isPending}>
          {save.isPending ? "Saving" : "Save"}
        </Button>
        <Button type="button" size="sm" variant="secondary" onClick={() => setEditing(null)}>
          Cancel
        </Button>
      </div>
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save the logins. They stay as they were. </span>
          {isApiError(save.error) ? save.error.message : "The logins could not be saved."}
        </Notice>
      )}
    </form>
  );
}
