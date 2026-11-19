/**
 * The Wiki page (pattern 11 of the design drafts): the tree of pages at the left, the page in the middle, and its
 * contents and properties in the right sidebar. A page is read drawn as Markdown; Edit opens the editor with a preview
 * beside it, History the commits that changed it, each of which opens the page as it was. Move renames a page or puts
 * it in another folder, Delete removes it and keeps its history. A viewer reads; a member writes.
 *
 * What is open is in the address: `?page=<path>`, with `&view=edit` or `&view=history`, `&at=<commit>` for an older
 * version, and `?new=<folder>` for a new page, so a page can be linked to and reloaded. The pages come from the wiki's
 * repository (`/v1/wiki/`), and `wiki_changed` reads them again when anyone edits one or the server brings what was
 * pushed to the repository from elsewhere.
 */
import { ClockCounterClockwiseIcon, PencilSimpleIcon, PlusIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { listProjects } from "../../api/calls/projects";
import { deleteWikiPage, moveWikiPage, wikiLinks, wikiPage, wikiPages } from "../../api/calls/wiki";
import { sayError } from "../../api/errorText";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { WikiPageView } from "../../api/types/wiki";
import { Markdown } from "../../ui/chat/Markdown";
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
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Empty } from "../../ui/page/Empty";
import { Field } from "../../ui/page/Field";
import { ListDetail } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { PageProperties, Property } from "../../ui/page/Properties";
import { SkeletonCard, SkeletonList } from "../../ui/page/Skeleton";
import { useRole } from "../shell/session";
import { linkPages } from "./links";
import { buildTree, folderOf, headings, PROJECTS, pathProblem } from "./model";
import { PageEditor } from "./PageEditor";
import { PageHistory } from "./PageHistory";
import { Backlinks } from "./WikiBacklinks";
import { WikiTree } from "./WikiTree";

export function WikiPage() {
  const queryClient = useQueryClient();
  const [params, setParams] = useSearchParams();
  const canWrite = useRole() !== "viewer";
  const pages = useQuery({ queryKey: keys.wikiPages, queryFn: wikiPages, retry: false });
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const projectKeys = (projects.data ?? []).map((project) => project.key);
  const tree = buildTree(pages.data ?? [], projects.data ?? []);

  const chosen = params.get("page") ?? pages.data?.[0]?.path ?? null;
  const newIn = params.get("new");
  const view = params.get("view");
  const at = params.get("at");
  const open = (next: Record<string, string>) => setParams(next, { replace: false });
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.wiki });

  const header = (
    <PageHeader
      title="Wiki"
      context="The organisation's pages, and a folder for each project. Every edit is a commit."
      actions={
        canWrite && pages.isSuccess ? (
          <Button size="sm" onClick={() => open({ new: "" })}>
            <PlusIcon className="size-4" weight="bold" />
            New page
          </Button>
        ) : undefined
      }
    />
  );

  if (pages.isError && isApiError(pages.error) && pages.error.status === 409) {
    return (
      <>
        {header}
        <Empty
          title="No wiki yet"
          body="The wiki is a git repository of Markdown pages. Name one under [wiki] url in moebius.toml, and the server keeps its pages here."
        />
      </>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {header}
      {pages.isError && (
        <Notice
          tone="error"
          action={
            <Button size="sm" variant="secondary" onClick={() => void pages.refetch()}>
              Try again
            </Button>
          }
        >
          {sayError(pages.error, "Could not read the wiki.")}
        </Notice>
      )}
      {pages.isPending && (
        <ListDetail
          list={<SkeletonList rows={5} message="Reading the wiki" />}
          detail={<SkeletonCard lines={6} />}
        />
      )}
      {pages.isSuccess && (
        <ListDetail
          list={
            <WikiTree
              tree={tree}
              chosen={newIn === null ? chosen : null}
              canWrite={canWrite}
              onChoose={(path) => open({ page: path })}
              onNew={(folder) => open({ new: folder })}
            />
          }
          detail={
            newIn !== null ? (
              <section aria-label="New page" className="flex min-w-0 flex-col gap-4">
                <h2 className="m-0 text-xl font-semibold text-text">New page</h2>
                <PageEditor
                  key={`new:${newIn}`}
                  path={null}
                  folder={newIn}
                  text=""
                  keys={projectKeys}
                  onSaved={(page) => {
                    void refresh();
                    open({ page: page.path });
                  }}
                  onCancel={() => open(chosen ? { page: chosen } : {})}
                />
              </section>
            ) : chosen === null ? (
              <Empty
                title="No page yet"
                body={
                  canWrite
                    ? "Write the first page: how the team works, a runbook, who is on call."
                    : "Nobody has written a page yet."
                }
                action={
                  canWrite ? (
                    <Button size="sm" onClick={() => open({ new: "" })}>
                      New page
                    </Button>
                  ) : undefined
                }
              />
            ) : (
              <ChosenPage
                key={`${chosen}@${at ?? ""}`}
                path={chosen}
                at={at}
                view={view}
                canWrite={canWrite}
                projectKeys={projectKeys}
                onView={(next, commit) =>
                  open({
                    page: chosen,
                    ...(next ? { view: next } : {}),
                    ...(commit ? { at: commit } : {}),
                  })
                }
                onMoved={(page) => {
                  void refresh();
                  open({ page: page.path });
                }}
                onDeleted={() => {
                  void refresh();
                  open({});
                }}
                onSaved={() => {
                  void refresh();
                  open({ page: chosen });
                }}
              />
            )
          }
        />
      )}
    </div>
  );
}

/** The page chosen: read, edited, or its history, with its contents and properties in the right sidebar. */
function ChosenPage({
  path,
  at,
  view,
  canWrite,
  projectKeys,
  onView,
  onMoved,
  onDeleted,
  onSaved,
}: {
  path: string;
  at: string | null;
  view: string | null;
  canWrite: boolean;
  projectKeys: readonly string[];
  onView: (view: "edit" | "history" | null, at?: string) => void;
  onMoved: (page: WikiPageView) => void;
  onDeleted: () => void;
  onSaved: () => void;
}) {
  const now = useNow();
  const article = useRef<HTMLDivElement>(null);
  const [moving, setMoving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const page = useQuery({
    queryKey: keys.wikiPage(path, at),
    queryFn: () => wikiPage(path, at ?? undefined),
    retry: false,
  });
  // What leads to and from the page is as it is now; an older version is drawn as it was written.
  const links = useQuery({ queryKey: keys.wikiLinks(path), queryFn: () => wikiLinks(path) });
  if (page.isPending) return <SkeletonCard lines={6} />;
  if (page.isError) {
    return (
      <Notice tone="error">
        {isApiError(page.error) && page.error.status === 404
          ? `There is no page ${path}.`
          : sayError(page.error, `Could not read ${path}.`)}
      </Notice>
    );
  }
  const shown = page.data;
  const contents = headings(shown.text);
  const project = path.startsWith(`${PROJECTS}/`) ? path.split("/")[1] : undefined;
  const jump = (index: number) => {
    const found = article.current?.querySelectorAll("h1, h2, h3")[index];
    found?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  return (
    <article aria-label={shown.name} className="flex min-w-0 flex-col gap-4">
      <PageProperties label={`Properties of ${shown.name}`}>
        {contents.length > 0 && (
          <nav aria-label="On this page" className="flex flex-col gap-0.5 pb-3">
            <span className="px-1 pb-1 text-sm font-semibold text-secondary">On this page</span>
            {contents.map((heading, index) => (
              <button
                // biome-ignore lint/suspicious/noArrayIndexKey: two headings can say the same, and the order is fixed
                key={index}
                type="button"
                onClick={() => jump(index)}
                style={{ paddingLeft: `${0.25 + (heading.level - 1) * 0.75}rem` }}
                className="min-h-7 cursor-pointer truncate rounded-control bg-transparent pr-1 text-left text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
              >
                {heading.text}
              </button>
            ))}
          </nav>
        )}
        <Property name="Edited by">{shown.version.author}</Property>
        <Property name="Edited">{ago(now, shown.version.at)}</Property>
        <Property name="In">{project ? `Project ${project}` : "Organisation"}</Property>
        <Property name="Path">
          <span className="font-mono text-sm">{shown.path}.md</span>
        </Property>
        <Backlinks links={links.data} />
      </PageProperties>

      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="m-0 break-words text-2xl font-semibold tracking-tight text-text">
            {shown.name}
          </h2>
          <p className="m-0 mt-1 text-sm text-secondary">
            Edited by {shown.version.author} · {ago(now, shown.version.at)}
            {folderOf(shown.path) !== "" && ` · in ${folderOf(shown.path)}`}
          </p>
        </div>
        {view !== "edit" && (
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="secondary"
              aria-pressed={view === "history"}
              onClick={() => onView(view === "history" ? null : "history", at ?? undefined)}
            >
              <ClockCounterClockwiseIcon className="size-4" aria-hidden />
              History
            </Button>
            {canWrite && at === null && (
              <>
                <Button size="sm" variant="secondary" onClick={() => setMoving(true)}>
                  Move
                </Button>
                <Button size="sm" variant="secondary" onClick={() => setDeleting(true)}>
                  Delete
                </Button>
                <Button size="sm" onClick={() => onView("edit")}>
                  <PencilSimpleIcon className="size-4" aria-hidden />
                  Edit
                </Button>
              </>
            )}
          </div>
        )}
      </header>

      {at !== null && (
        <Notice
          action={
            <Button size="sm" variant="secondary" onClick={() => onView(null)}>
              Read the page as it is
            </Button>
          }
        >
          {`An older version: ${shown.version.message}, by ${shown.version.author}, ${ago(now, shown.version.at)}.`}
        </Notice>
      )}

      {view === "edit" && at === null ? (
        <PageEditor
          path={path}
          folder={folderOf(path)}
          text={shown.text}
          keys={projectKeys}
          onSaved={onSaved}
          onCancel={() => onView(null)}
        />
      ) : (
        <div className="flex min-w-0 flex-col gap-6">
          {view === "history" && (
            <PageHistory path={path} at={at} onOpen={(commit) => onView("history", commit)} />
          )}
          <div ref={article} className="min-w-0">
            {shown.text.trim() === "" ? (
              <p className="m-0 text-base text-secondary">This page is empty.</p>
            ) : (
              <Markdown
                content={
                  at === null ? linkPages(shown.text, links.data?.links.pages ?? []) : shown.text
                }
                className="max-w-prose"
              />
            )}
          </div>
        </div>
      )}

      <MoveDialog
        open={moving}
        onOpenChange={setMoving}
        from={path}
        projectKeys={projectKeys}
        onMoved={onMoved}
      />
      <ConfirmDelete
        open={deleting}
        onOpenChange={setDeleting}
        title={`Delete ${shown.name}?`}
        onConfirm={async () => {
          await deleteWikiPage(path);
          onDeleted();
        }}
      >
        The page leaves the wiki. Its history stays in the repository.
      </ConfirmDelete>
    </article>
  );
}

/** Asks for the page's new path: another name renames it, another folder moves it. */
function MoveDialog({
  open,
  onOpenChange,
  from,
  projectKeys,
  onMoved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  from: string;
  projectKeys: readonly string[];
  onMoved: (page: WikiPageView) => void;
}) {
  const [to, setTo] = useState(from);
  const move = useMutation({
    mutationFn: () => moveWikiPage(from, to.trim()),
    onSuccess: (page) => {
      onOpenChange(false);
      onMoved(page);
    },
  });
  const problem = pathProblem(to.trim(), projectKeys);
  const ready = problem === null && to.trim() !== from && !move.isPending;
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          setTo(from);
          move.reset();
        }
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (ready) move.mutate();
          }}
        >
          <DialogHeader>
            <DialogTitle>Move the page</DialogTitle>
            <DialogDescription>
              Another name renames it; another folder moves it. Its history goes with it.
            </DialogDescription>
          </DialogHeader>
          <Field label="Path" error={to.trim() !== from && problem ? problem : undefined}>
            <Input
              value={to}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setTo(event.target.value)}
            />
          </Field>
          {move.isError && (
            <Notice tone="error">
              <span className="font-semibold">Couldn't move the page. </span>
              {sayError(move.error, "Try again.")}
            </Notice>
          )}
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={!ready}>
              {move.isPending ? "Moving" : "Move"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
