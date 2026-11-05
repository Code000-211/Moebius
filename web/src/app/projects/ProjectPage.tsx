/**
 * The page of one repository (`/projects/:id`; the desktop app's `screens/project`): where it is and what it trusts,
 * with the logins of a repository on a forge to edit, the settings Moebius keeps for it, the settings plugins
 * ask of it (only in the web app), and its files on the default branch, which can be read, edited and saved as a
 * commit. The parts are borderless cards grouped by space. The board of the repository is its own
 * page.
 */
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import { editTrusted } from "../../api/calls/projects";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Repository } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { FilesPanel } from "./FilesPanel";
import { forgeTag, logins, onForge } from "./model";
import { PluginSettings } from "./PluginSettings";
import { SettingsPanel } from "./SettingsPanel";

export function ProjectPage() {
  const { id } = useParams();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  if (data.isPending)
    return (
      <>
        <PageHeader title="Project" />
        <SkeletonCard lines={4} message="Reading the project" />
      </>
    );
  const board = data.data?.boards.find((each) => String(each.repository.id) === id);
  // A read that failed is not a repository that has gone: saying so comes before the page that says it is not here.
  if (data.isError) {
    return (
      <>
        <PageHeader title="Project" />
        <ReadFailed
          error={data.error}
          fallback="The project could not be read. Try again."
          busy={data.isFetching}
          onRetry={() => void data.refetch()}
        />
      </>
    );
  }
  if (!board) {
    return (
      <>
        <PageHeader title="Project" />
        <Empty
          title="No such repository"
          body="This server has no repository with that address. It may have been removed."
          action={
            <Button asChild size="sm">
              <Link to="/projects">Choose a repository</Link>
            </Button>
          }
        />
      </>
    );
  }
  const { repository } = board;
  return (
    <>
      <PageHeader
        title={repository.name}
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
          <Button asChild size="sm" variant="secondary">
            <Link to={boardPath(repository.id)}>Open board</Link>
          </Button>
        }
      />
      <div className="flex flex-col gap-6">
        <Box title="Settings">
          <dl className="m-0 mb-4 flex flex-wrap gap-4">
            <Fact label="Forge" value={forgeTag(repository)} />
            <Fact label="Default branch" value={repository.default_branch} mono />
            <Fact label="Clone on the server" value={repository.path} mono />
          </dl>
          {onForge(repository) ? (
            <Trusted key={repository.trusted.join(",")} repository={repository} />
          ) : (
            <p className="m-0 max-w-prose text-base text-secondary">
              This repository is on no forge, so it has no trusted authors: its issues come from
              here.
            </p>
          )}
        </Box>
        <PluginSettings repository={repository.id} />
        <FilesPanel repository={repository.id} />
        <SettingsPanel repository={repository.id} />
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
