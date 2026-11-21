/**
 * The Projects page (pattern 21 of the design drafts): every project as a tile, with its key, its repositories, the
 * people who work in it and its open cards. A tile opens the project's settings in tabs (`ProjectSettingsPage`); the
 * board is in the sidebar's project switcher. "New project" makes a project with a name and a key the server checks is
 * free, whose cards are worked on in its own files until it has a repository; "Add a repository" makes a repository
 * with a project of its own, as every repository has.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { addProject, listProjects } from "../../api/calls/projects";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Input } from "../../ui/components/Input";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { settingsPath } from "../pages";
import { isKey, keyFor, repositoriesText } from "./model";

/** The category colours a project's mark and a member's initial take, by id, so each keeps its colour. */
const HUES = ["bg-cat-1", "bg-cat-2", "bg-cat-3", "bg-cat-4", "bg-cat-5", "bg-cat-6"];
const hue = (id: number) => HUES[(id - 1 + HUES.length) % HUES.length];

/** How many members a tile shows by their initial before it counts the rest. */
const SHOWN_MEMBERS = 4;

export function ProjectsPage() {
  const data = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const [making, setMaking] = useState(false);

  const header = (
    <PageHeader
      title="Projects"
      actions={
        <>
          <Button asChild size="sm" variant="secondary">
            <Link to="/projects/new">
              <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
              Add a repository
            </Link>
          </Button>
          <Button size="sm" onClick={() => setMaking(true)}>
            <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
            New project
          </Button>
        </>
      }
    />
  );

  if (data.isPending)
    return (
      <>
        <PageHeader title="Projects" />
        <SkeletonList rows={3} message="Reading the projects" />
      </>
    );
  // A read that failed is not a server with no project: the empty state below would say there is nothing to open.
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
  const projects = data.data;

  return (
    <>
      {header}
      {projects.length === 0 ? (
        <Empty
          title="No project yet"
          body="Add a repository and Moebius clones it on the server and makes its project. Its board opens as soon as the clone is there, and cards come from the issues you pick."
          action={
            <Button asChild size="sm">
              <Link to="/projects/new">Add a repository</Link>
            </Button>
          }
        />
      ) : (
        <ul className="m-0 list-none border-t border-hairline p-0" aria-label="Projects">
          {projects.map((project) => (
            <Row key={project.id} project={project} />
          ))}
        </ul>
      )}
      <NewProject open={making} onOpenChange={setMaking} />
    </>
  );
}

/** One project as a row of 36px: its mark, name and key, its repositories, its members and its open cards. It opens the project's settings. */
function Row({ project }: { project: Project }) {
  return (
    <li data-testid="project" className="min-w-0">
      <Link
        to={settingsPath(project.id)}
        aria-label={`${project.name}, ${project.key}`}
        className="flex h-9 min-w-0 items-center gap-2.5 border-b border-hairline px-4 text-base text-text no-underline hover:bg-row-hover"
      >
        <span
          aria-hidden
          className={cn(
            "inline-grid size-4 shrink-0 place-items-center rounded-control text-2xs font-semibold text-on-ink",
            hue(project.id),
          )}
        >
          {project.key[0]}
        </span>
        <span className="min-w-0 shrink truncate font-medium">{project.name}</span>
        <span className="shrink-0 font-mono text-xs text-muted">{project.key}</span>
        <span className="min-w-0 flex-1 truncate text-muted">{repositoriesText(project)}</span>
        <Members project={project} />
        <span className="w-16 shrink-0 text-right text-xs text-muted tabular-nums">{`${project.open_cards} open`}</span>
      </Link>
    </li>
  );
}

/** The first members of a project by their initial, and how many more there are. */
function Members({ project }: { project: Project }) {
  const shown = project.members.slice(0, SHOWN_MEMBERS);
  const more = project.members.length - shown.length;
  return (
    <span className="flex shrink-0 items-center gap-1">
      {shown.map((member) => (
        <Tooltip key={member.id} content={`${member.name} (@${member.handle})`}>
          <span
            role="img"
            aria-label={member.name}
            className={cn(
              "inline-grid size-5 place-items-center rounded-full text-xs font-semibold text-on-ink",
              hue(member.id),
            )}
          >
            {(member.name[0] ?? member.handle[0] ?? "?").toUpperCase()}
          </span>
        </Tooltip>
      ))}
      {more > 0 && <span className="text-sm text-secondary">{`+${more}`}</span>}
    </span>
  );
}

/** The dialog that makes a project: its name, and a key offered from the name until the person types their own. */
function NewProject({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [typedKey, setTypedKey] = useState<string | null>(null);
  const key = typedKey ?? keyFor(name);
  const make = useMutation({
    mutationFn: addProject,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: keys.projects });
      close(false);
    },
  });
  function close(next: boolean) {
    if (!next) {
      setName("");
      setTypedKey(null);
      make.reset();
    }
    onOpenChange(next);
  }
  const ready = name.trim() !== "" && isKey(key) && !make.isPending;
  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (ready) make.mutate({ name: name.trim(), key });
          }}
        >
          <DialogHeader>
            <DialogTitle>New project</DialogTitle>
            <DialogDescription>
              A board of its own. Its key names its cards, such as {`${key || "WEB"}-1`}, and stays
              when the project is renamed.
            </DialogDescription>
          </DialogHeader>
          <Field label="Name">
            <Input
              value={name}
              placeholder="Storefront"
              autoComplete="off"
              onChange={(event) => setName(event.target.value)}
            />
          </Field>
          <Field
            label="Key"
            hint="An uppercase letter, then 1 to 9 uppercase letters or digits."
            error={key !== "" && !isKey(key) ? "This is not a key yet." : undefined}
          >
            <Input
              value={key}
              placeholder="SHOP"
              autoComplete="off"
              spellCheck={false}
              mono
              onChange={(event) => setTypedKey(event.target.value.toUpperCase())}
            />
          </Field>
          {make.isError && (
            <Notice tone="error">
              <span className="font-semibold">Couldn't make the project. </span>
              {sayError(make.error, "Try again.")}
            </Notice>
          )}
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={() => close(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!ready}>
              {make.isPending ? "Making" : "Make project"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
