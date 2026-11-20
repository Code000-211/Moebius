/**
 * The Wiki page: three columns inside the panel, the tree of pages at the left, the page in the middle, and the
 * outline of its headings (with what links to it) at the right, divided by hairlines and each scrolling on its own. A page is read drawn as Markdown; Edit opens the editor with a preview
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
import { type ReactNode, useRef, useState } from "react";
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
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard, SkeletonList } from "../../ui/page/Skeleton";
import { useToast } from "../../ui/page/Toast";
import { useRole } from "../shell/session";
import { linkPages } from "./links";
import { buildTree, folderOf, headings, pathProblem } from "./model";
import { PageEditor } from "./PageEditor";
import { PageHistory } from "./PageHistory";
import { Backlinks } from "./WikiBacklinks";
import { WikiSearchBox, WikiSearchResults } from "./WikiSearch";
import { FromTemplate, TemplatesDialog } from "./WikiTemplates";
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
  // A viewer has no form for a new page, whatever the address says: the server refuses the save.
  const newIn = canWrite ? params.get("new") : null;
  const view = params.get("view");
  const at = params.get("at");
  const [search, setSearch] = useState("");
  const [templating, setTemplating] = useState(false);
  const open = (next: Record<string, string>) => setParams(next, { replace: false });
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.wiki });

  const header = (
    <PageHeader
      title="Wiki"
      actions={
        canWrite && pages.isSuccess ? (
          <>
            <Button size="sm" variant="ghost" onClick={() => setTemplating(true)}>
              Templates
            </Button>
            <Button size="sm" onClick={() => open({ new: "" })}>
              <PlusIcon className="size-4" aria-hidden />
              New page
            </Button>
          </>
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
      <TemplatesDialog open={templating} onOpenChange={setTemplating} />
      {pages.isPending && (
        <Columns
          tree={<SkeletonList rows={5} message="Reading the wiki" />}
          text={
            <TextColumn>
              <SkeletonCard lines={6} />
            </TextColumn>
          }
        />
      )}
      {pages.isSuccess && (
        <Columns
          tree={
            <div className="flex flex-col gap-3">
              <WikiSearchBox value={search} onChange={setSearch} />
              {search.trim() !== "" ? (
                <WikiSearchResults
                  query={search}
                  onOpen={(path) => {
                    setSearch("");
                    open({ page: path });
                  }}
                />
              ) : (
                <WikiTree
                  tree={tree}
                  chosen={newIn === null ? chosen : null}
                  canWrite={canWrite}
                  onChoose={(path) => open({ page: path })}
                  onNew={(folder) => open({ new: folder })}
                />
              )}
            </div>
          }
          text={
            newIn !== null ? (
              <TextColumn>
                <section aria-label="New page" className="flex min-w-0 flex-col gap-4">
                  <h2 className="m-0 text-xl font-semibold tracking-tight text-text">New page</h2>
                  <FromTemplate
                    key={`template:${newIn}`}
                    folder={newIn}
                    keys={projectKeys}
                    onMade={(page) => {
                      void refresh();
                      open({ page: page.path });
                    }}
                  />
                  <PageEditor
                    key={`new:${newIn}`}
                    path={null}
                    folder={newIn}
                    text=""
                    version={null}
                    keys={projectKeys}
                    onSaved={(page) => {
                      void refresh();
                      open({ page: page.path });
                    }}
                    onCancel={() => open(chosen ? { page: chosen } : {})}
                  />
                </section>
              </TextColumn>
            ) : chosen === null ? (
              <TextColumn>
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
              </TextColumn>
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

/** The tree at the left and, after it, the columns of the page; they fill the panel from edge to edge. */
function Columns({ tree, text }: { tree: ReactNode; text: ReactNode }) {
  return (
    <div className="-mx-6 -mt-6 -mb-6 flex min-h-0 flex-1 flex-col lg:flex-row">
      <div
        data-testid="list-column"
        className="flex w-full shrink-0 flex-col gap-2 border-b border-hairline p-3 lg:min-h-0 lg:w-60 lg:overflow-y-auto lg:border-r lg:border-b-0"
      >
        {tree}
      </div>
      {text}
    </div>
  );
}

/** The middle column: the text of a page, a new page or the empty state. */
function TextColumn({ children }: { children: ReactNode }) {
  return (
    <div
      data-testid="detail-column"
      className="min-w-0 flex-1 px-8 py-6 lg:min-h-0 lg:overflow-y-auto"
    >
      {children}
    </div>
  );
}

/** The page chosen: read, edited, or its history, in the middle column, with its outline in the column at the right. */
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
  if (page.isPending) {
    return (
      <TextColumn>
        <SkeletonCard lines={6} />
      </TextColumn>
    );
  }
  if (page.isError) {
    return (
      <TextColumn>
        <Notice tone="error">
          {isApiError(page.error) && page.error.status === 404
            ? `There is no page ${path}.`
            : sayError(page.error, `Could not read ${path}.`)}
        </Notice>
      </TextColumn>
    );
  }
  const shown = page.data;
  // The page's own title is the first line of its text, and the header above it already says it.
  const all = headings(shown.text);
  const skipTitle = all[0]?.level === 1 && all[0].text === shown.name ? 1 : 0;
  const contents = all.slice(skipTitle);
  const top = Math.min(...contents.map((heading) => heading.level));
  const jump = (index: number) => {
    const found = article.current?.querySelectorAll("h1, h2, h3")[index + skipTitle];
    found?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  const editing = view === "edit" && at === null;
  const hasLinks =
    (links.data?.backlinks.pages.length ?? 0) + (links.data?.backlinks.cards.length ?? 0) > 0;

  return (
    <>
      <TextColumn>
        <article aria-label={shown.name} className="flex min-w-0 flex-col gap-5">
          <header className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
            <div className="min-w-0">
              <h2 className="m-0 break-words text-xl font-semibold tracking-tight text-text">
                {shown.name}
              </h2>
              <p className="m-0 mt-1 text-xs text-muted">
                {shown.version.author} · {ago(now, shown.version.at)}
                {folderOf(shown.path) !== "" && ` · ${folderOf(shown.path)}`}
              </p>
            </div>
            {view !== "edit" && (
              <div className="flex flex-wrap items-center gap-1">
                <Button
                  size="sm"
                  variant="ghost"
                  aria-pressed={view === "history"}
                  onClick={() => onView(view === "history" ? null : "history", at ?? undefined)}
                >
                  <ClockCounterClockwiseIcon className="size-4" aria-hidden />
                  History
                </Button>
                {canWrite && at === null && (
                  <>
                    <Button size="sm" variant="ghost" onClick={() => setMoving(true)}>
                      Move
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      className="text-error-text"
                      onClick={() => setDeleting(true)}
                    >
                      Delete
                    </Button>
                    <Button size="sm" variant="secondary" onClick={() => onView("edit")}>
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

          {editing ? (
            <PageEditor
              path={path}
              folder={folderOf(path)}
              text={shown.text}
              version={shown.version.commit}
              latest={shown}
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
                  <p className="m-0 text-base text-muted">This page is empty.</p>
                ) : (
                  <Markdown
                    content={
                      at === null
                        ? linkPages(shown.text, links.data?.links.pages ?? [])
                        : shown.text
                    }
                    className="max-w-prose"
                  />
                )}
              </div>
            </div>
          )}
        </article>
      </TextColumn>

      {(contents.length > 0 || hasLinks) && (
        <aside
          data-testid="outline-column"
          className="hidden w-56 shrink-0 flex-col gap-4 overflow-y-auto border-l border-hairline px-3 py-4 lg:flex"
        >
          {contents.length > 0 && (
            <nav aria-label="On this page" className="flex flex-col">
              {contents.map((heading, index) => (
                <button
                  // biome-ignore lint/suspicious/noArrayIndexKey: two headings can say the same, and the order is fixed
                  key={index}
                  type="button"
                  onClick={() => jump(index)}
                  style={{ paddingLeft: `${0.5 + (heading.level - top) * 0.75}rem` }}
                  className="min-h-7 cursor-pointer truncate rounded-control bg-transparent pr-2 text-left text-sm text-muted transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
                >
                  {heading.text}
                </button>
              ))}
            </nav>
          )}
          <Backlinks links={links.data} />
        </aside>
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
    </>
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
  const toast = useToast();
  const move = useMutation({
    mutationFn: () => moveWikiPage(from, to.trim()),
    onSuccess: (page) => {
      onOpenChange(false);
      onMoved(page);
    },
    onError: (error) =>
      toast({ kind: "error", text: `Couldn't move the page. ${sayError(error, "Try again.")}` }),
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
