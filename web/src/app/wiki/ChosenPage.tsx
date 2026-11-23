/**
 * The page chosen on the Wiki page or a project's Wiki tab: read, edited, or its history, in the middle column, with its
 * outline in the column at the right. Its head says who edited it when and its category, and carries its labels
 * (`PageLabels`); Move puts it in another category or renames it, Delete removes it and keeps its history.
 */
import { ClockCounterClockwiseIcon, PencilSimpleIcon } from "@phosphor-icons/react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useRef, useState } from "react";
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
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { useToast } from "../../ui/page/Toast";
import { CategoryPicker } from "./CategoryPicker";
import { linkPages } from "./links";
import {
  categoriesOf,
  categoryOf,
  folderOf,
  groupOf,
  headings,
  joinPath,
  pathProblem,
} from "./model";
import { PageEditor } from "./PageEditor";
import { PageHistory } from "./PageHistory";
import { PageLabels } from "./PageLabels";
import { TextColumn } from "./TextColumn";
import { Backlinks } from "./WikiBacklinks";

/** The page chosen: read, edited, or its history, in the middle column, with its outline in the column at the right. */
export function ChosenPage({
  path,
  at,
  view,
  canWrite,
  projectKeys,
  known,
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
  /** The labels in use, offered when one is added. */
  known: readonly string[];
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
  const all = headings(shown.body);
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
              <p className="m-0 mt-1 mb-2 text-xs text-muted">
                {shown.version.author} · {ago(now, shown.version.at)}
                {categoryOf(shown.path) !== "" && ` · ${categoryOf(shown.path)}`}
              </p>
              <PageLabels
                page={shown}
                editable={canWrite && at === null && !editing}
                known={known}
              />
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
                {shown.body.trim() === "" ? (
                  <p className="m-0 text-base text-muted">This page is empty.</p>
                ) : (
                  <Markdown
                    content={
                      at === null
                        ? linkPages(shown.body, links.data?.links.pages ?? [])
                        : shown.body
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

/** Asks where the page goes: another category of its group, another name, or both. A slash in the name makes a folder. */
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
  const group = groupOf(from);
  const start = { category: categoryOf(from), name: from.slice(from.lastIndexOf("/") + 1) };
  const [category, setCategory] = useState(start.category);
  const [name, setName] = useState(start.name);
  const toast = useToast();
  const pages = useQuery({ queryKey: keys.wikiPages, queryFn: wikiPages, retry: false });
  const to = joinPath(group, category.trim(), name.trim());
  const move = useMutation({
    mutationFn: () => moveWikiPage(from, to),
    onSuccess: (page) => {
      onOpenChange(false);
      onMoved(page);
    },
    onError: (error) =>
      toast({ kind: "error", text: `Couldn't move the page. ${sayError(error, "Try again.")}` }),
  });
  const problem = pathProblem(to, projectKeys);
  const ready = problem === null && to !== from && !move.isPending;
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          setCategory(start.category);
          setName(start.name);
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
              Another category moves it; another name renames it. Its history goes with it.
            </DialogDescription>
          </DialogHeader>
          <CategoryPicker
            categories={categoriesOf(pages.data ?? [], group)}
            value={category}
            onChange={setCategory}
            problem={problem}
          />
          <Field
            label="Name"
            hint="A slash makes a folder: Team/On call."
            error={to !== from && problem ? problem : undefined}
          >
            <Input
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setName(event.target.value)}
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
