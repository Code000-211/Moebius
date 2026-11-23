/**
 * What the Wiki page and a project's Wiki tab share: three columns inside the panel, the tree of pages at the left
 * (above it the search box and the label chips), the page in the middle, and the outline of its headings (with what links
 * to it) at the right, divided by hairlines and each scrolling on its own. The header is the caller's, given the
 * controls it needs (`header`), because the Wiki page draws a title and a project the tabs of the project.
 *
 * The scope is what is shown: everything (the organisation's pages and a group for each project), or one project's group
 * `projects/<KEY>/`, whose categories and labels are the only ones offered. What is open is in the address:
 * `?page=<path>`, with `&view=edit` or `&view=history`, `&at=<commit>` for an older version, and `?new=<folder>` for a new
 * page, so a page can be linked to and reloaded. The labels chosen to filter the tree are the workspace's own and are
 * forgotten on leaving. For an owner a notice above the columns says where the pages are kept and offers a git
 * repository (`WikiRepositoryNotice`), also in a project's tab. The pages come from the wiki's repository (`/v1/wiki/`), and `wiki_changed` reads them again
 * when anyone edits one or the server brings what was pushed to the repository from elsewhere.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { useSearchParams } from "react-router";
import { listProjects } from "../../api/calls/projects";
import { wikiPages } from "../../api/calls/wiki";
import { sayError } from "../../api/errorText";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { SkeletonCard, SkeletonList } from "../../ui/page/Skeleton";
import { useRole } from "../shell/session";
import { ChosenPage } from "./ChosenPage";
import { LabelFilter } from "./LabelFilter";
import { buildTree, labelsInUse, PROJECTS, pagesOfGroup, withLabels } from "./model";
import { NewPage } from "./NewPage";
import { TextColumn } from "./TextColumn";
import { WikiRepositoryNotice } from "./WikiRepository";
import { WikiSearchBox, WikiSearchResults } from "./WikiSearch";
import { TemplatesDialog } from "./WikiTemplates";
import { WikiTree } from "./WikiTree";

/** What the workspace shows: everything, or one project's pages. */
export type WikiScope = { kind: "all" } | { kind: "project"; key: string; name: string };

/** What the caller's header needs to draw its buttons. */
export interface WikiHeaderControls {
  canWrite: boolean;
  /** Whether the pages are read, so the buttons make sense. */
  ready: boolean;
  /** Starts a new page, in the scope's own group. */
  onNew: () => void;
  onTemplates: () => void;
}

export function WikiWorkspace({
  scope,
  header,
}: {
  scope: WikiScope;
  header: (controls: WikiHeaderControls) => ReactNode;
}) {
  const queryClient = useQueryClient();
  const [params, setParams] = useSearchParams();
  const canWrite = useRole() !== "viewer";
  const pages = useQuery({ queryKey: keys.wikiPages, queryFn: wikiPages, retry: false });
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const projectKeys = (projects.data ?? []).map((project) => project.key);
  const group = scope.kind === "project" ? `${PROJECTS}/${scope.key}` : null;
  const [chosenLabels, setChosenLabels] = useState<string[]>([]);

  // The pages of the scope, the labels in use among them, and the tree of those that carry the labels chosen.
  const inScope = group === null ? (pages.data ?? []) : pagesOfGroup(pages.data ?? [], group);
  const labels = labelsInUse(inScope);
  // A label chosen that no page carries any more (it was removed) filters nothing out of thin air.
  const chosenNow = chosenLabels.filter((label) => labels.some((each) => each.label === label));
  const shown = withLabels(inScope, chosenNow);
  const built = buildTree(
    shown,
    scope.kind === "project" ? [{ key: scope.key, name: scope.name }] : (projects.data ?? []),
  );
  const tree =
    scope.kind === "project"
      ? { organisation: [], projects: built.projects.filter((each) => each.key === scope.key) }
      : built;

  const chosen = params.get("page") ?? inScope[0]?.path ?? null;
  // A viewer has no form for a new page, whatever the address says: the server refuses the save.
  const asked = canWrite ? params.get("new") : null;
  const newIn = asked === "" && group !== null ? group : asked;
  const view = params.get("view");
  const at = params.get("at");
  const [search, setSearch] = useState("");
  const [templating, setTemplating] = useState(false);
  const open = (next: Record<string, string>) => setParams(next, { replace: false });
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.wiki });
  const controls: WikiHeaderControls = {
    canWrite,
    ready: pages.isSuccess,
    onNew: () => open({ new: group ?? "" }),
    onTemplates: () => setTemplating(true),
  };

  if (pages.isError && isApiError(pages.error) && pages.error.status === 409) {
    return (
      <>
        {header(controls)}
        <WikiRepositoryNotice />
        <Empty
          title="The wiki is not open"
          body={sayError(pages.error, "The server could not open its wiki.")}
        />
      </>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {header(controls)}
      <WikiRepositoryNotice />
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
                  project={scope.kind === "project" ? scope.key : undefined}
                  onOpen={(path) => {
                    setSearch("");
                    open({ page: path });
                  }}
                />
              ) : (
                <>
                  <LabelFilter labels={labels} chosen={chosenNow} onChange={setChosenLabels} />
                  <WikiTree
                    tree={tree}
                    chosen={newIn === null ? chosen : null}
                    canWrite={canWrite}
                    organisation={scope.kind === "all"}
                    filtered={chosenNow.length > 0}
                    onChoose={(path) => open({ page: path })}
                    onNew={(folder) => open({ new: folder })}
                  />
                </>
              )}
            </div>
          }
          text={
            newIn !== null ? (
              <TextColumn>
                <NewPage
                  key={`new:${newIn}`}
                  folder={newIn}
                  pages={pages.data}
                  keys={projectKeys}
                  onMade={(page) => {
                    void refresh();
                    open({ page: page.path });
                  }}
                  onCancel={() => open(chosen ? { page: chosen } : {})}
                />
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
                      <Button size="sm" onClick={controls.onNew}>
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
                known={labels.map((each) => each.label)}
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
    <div className="-mx-4 -mt-4 -mb-4 flex min-h-0 flex-1 flex-col lg:flex-row">
      <div
        data-testid="list-column"
        className="flex w-full shrink-0 flex-col gap-2 border-b border-hairline px-4 py-3 lg:min-h-0 lg:w-60 lg:overflow-y-auto lg:border-r lg:border-b-0"
      >
        {tree}
      </div>
      {text}
    </div>
  );
}
