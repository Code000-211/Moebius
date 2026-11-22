/**
 * One workflow, as one page: a header with the way back (`<Project> › Workflows ›`), the workflow's name, a chip for
 * what starts it, the tabs Overview | Canvas | Runs | File (each at an address of its own, so a link and a reload keep
 * it), and at the right Copy hook URL (a webhook's only), a `...` menu with Delete, and the primary Run.
 *
 * - Overview (`WorkflowOverview.tsx`): the trigger and last run, the steps in plain words, the last few runs.
 * - Canvas (`canvas/WorkflowCanvasPage.tsx`): the editor, with the pane of a chosen step beside the graph.
 * - Runs (`WorkflowRuns.tsx`): every run, each opening its page.
 * - File: the TOML exactly as stored, read-only, scrolling inside its box, with Copy.
 *
 * It reads `GET /v1/projects/{id}/workflows/{name}` and the project's list (which holds the next firing of a schedule
 * and the address of a webhook). A definition that could not be read says so and offers to read it again. A workflow
 * with no file yet — the blank draft the "New workflow" dialog passes as router state, or an address reloaded before
 * the first save — has only the canvas until that save creates it. Delete is `DELETE /v1/projects/{id}/workflows/{name}`,
 * asked once: the dialog warns that a workflow shaped like an automation disappears from the Scheduled list too, and a
 * refusal of the server (a run queued, running or waiting) is shown in it in the server's own words.
 */
import { DotsThreeIcon, PlayIcon, TrashIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router";
import { listProjects } from "../../api/calls/projects";
import {
  deleteProjectWorkflow,
  projectWorkflow,
  projectWorkflows,
} from "../../api/calls/workflows";
import { sayError } from "../../api/errorText";
import { ApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { CodeBlock } from "../../ui/chat/CodeBlock";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../../ui/components/DropdownMenu";
import { useNow } from "../../ui/lib/useNow";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { boardPath, projectTabPath, WORKFLOW_TABS, type WorkflowTab, workflowPath } from "../pages";
import { newWorkflowDraft } from "./canvas/model";
import { draftOf, WorkflowCanvas } from "./canvas/WorkflowCanvasPage";
import { CopyHookUrl } from "./HookAddress";
import { looksLikeAutomation } from "./model";
import { RunWorkflowDialog } from "./RunWorkflowDialog";
import { triggerChip } from "./triggerWords";
import { WorkflowOverview } from "./WorkflowOverview";
import { WorkflowRuns } from "./WorkflowRuns";

const LABELS: Record<WorkflowTab, string> = {
  overview: "Overview",
  canvas: "Canvas",
  runs: "Runs",
  file: "File",
};

export function WorkflowPage() {
  const { projectId = "", name = "", tab: asked } = useParams();
  const project = Number(projectId);
  const tab: WorkflowTab = WORKFLOW_TABS.find((each) => each === asked) ?? "overview";
  const navigate = useNavigate();
  const location = useLocation();
  const queryClient = useQueryClient();
  const base = useId();
  const now = useNow();
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [removeRefusal, setRemoveRefusal] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  // The blank graph the "New workflow" dialog starts is passed as router state: there is no file to read for it.
  const incoming = draftOf(location.state);
  const [created, setCreated] = useState(false);
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const owner = projects.data?.find((each) => each.id === project);
  const definition = useQuery({
    queryKey: keys.projectWorkflow(project, name),
    queryFn: () => projectWorkflow(project, name),
    enabled: incoming === null || created,
    // A name no file holds yet is a new workflow, so a read that finds none is not tried again.
    retry: (count, error) => !(error instanceof ApiError && error.status === 404) && count < 3,
  });
  const listed = useQuery({
    queryKey: keys.projectWorkflows(project),
    queryFn: () => projectWorkflows(project),
  });
  const summary = listed.data?.find((each) => each.name === name);
  const remove = useMutation({
    mutationFn: () => deleteProjectWorkflow(project, definition.data?.name ?? name),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: keys.workflows });
      await queryClient.invalidateQueries({ queryKey: keys.automations });
      queryClient.removeQueries({ queryKey: keys.projectWorkflow(project, name) });
      queryClient.removeQueries({ queryKey: keys.projectWorkflowRuns(project, name) });
      navigate(projectTabPath(project, "workflows"));
    },
    onError: (error) => setRemoveRefusal(sayError(error, "Could not delete it. Try again.")),
  });

  const ownerName = definition.data?.project_name ?? owner?.name ?? "Project";
  const trail = (
    <nav aria-label="Where this is" className="flex items-center gap-1.5 text-sm text-secondary">
      <Link className="rounded-round underline-offset-4 hover:underline" to={boardPath(project)}>
        {ownerName}
      </Link>
      <span aria-hidden="true">›</span>
      <Link
        className="rounded-round underline-offset-4 hover:underline"
        to={projectTabPath(project, "workflows")}
      >
        Workflows
      </Link>
      <span aria-hidden="true">›</span>
    </nav>
  );

  const notFound =
    definition.isError && definition.error instanceof ApiError && definition.error.status === 404;
  const draft = incoming ?? (notFound ? newWorkflowDraft(name, "") : null);
  const saved = definition.data?.definition ?? draft;
  if (saved === null) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <PageHeader title={name} back={trail} />
        {definition.isError ? (
          <ReadFailed
            error={definition.error}
            fallback="Could not read this workflow's definition. Try again."
            busy={definition.isFetching}
            onRetry={() => void definition.refetch()}
          />
        ) : (
          <SkeletonLines lines={8} message="Reading the definition" />
        )}
      </div>
    );
  }
  // No file yet: only the canvas, which creates it with its first save.
  const fresh = draft !== null && !created && definition.data === undefined;
  const file = definition.data?.file ?? "";
  const kind = saved.trigger[0]?.kind;
  const leave = () => {
    remove.reset();
    setRemoveRefusal(null);
    setConfirmingDelete(false);
  };
  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title={saved.name}
        back={trail}
        badge={fresh ? undefined : <Tag>{triggerChip(kind, summary, now)}</Tag>}
        tabs={
          fresh ? undefined : (
            <Tabs
              tabs={WORKFLOW_TABS.map((each) => ({ value: each, label: LABELS[each] }))}
              value={tab}
              onChange={(next) => navigate(workflowPath(project, saved.name, next))}
              label="Workflow"
              idBase={base}
            />
          )
        }
        actions={
          fresh || summary?.hook_address === undefined ? null : (
            <CopyHookUrl address={summary.hook_address} />
          )
        }
        menu={
          fresh ? undefined : (
            <DropdownMenu>
              <DropdownMenuTrigger
                aria-label="More workflow actions"
                className="inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-control bg-transparent text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text data-[state=open]:bg-hover data-[state=open]:text-text"
              >
                <DotsThreeIcon className="size-5" weight="bold" aria-hidden="true" />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  className="text-danger-text"
                  onSelect={() => setConfirmingDelete(true)}
                >
                  <TrashIcon className="size-4" aria-hidden="true" />
                  Delete workflow
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )
        }
        primary={
          fresh ? undefined : (
            <Button variant="primary" size="sm" disabled={!owner} onClick={() => setRunning(true)}>
              <PlayIcon className="size-4" aria-hidden="true" />
              Run
            </Button>
          )
        }
      />
      {owner ? (
        <RunWorkflowDialog
          project={owner}
          name={saved.name}
          open={running}
          onOpenChange={setRunning}
        />
      ) : null}
      {confirmingDelete && (
        <Dialog open onOpenChange={(open) => !open && leave()}>
          <DialogContent>
            <form
              className="flex flex-col gap-4"
              onSubmit={(event) => {
                event.preventDefault();
                if (!remove.isPending) remove.mutate();
              }}
            >
              <DialogHeader>
                <DialogTitle>{`Delete ${saved.name}?`}</DialogTitle>
                <DialogDescription>
                  The workflow's file is deleted and committed. Its past runs stay listed.
                  {looksLikeAutomation(saved) &&
                    " This workflow is an automation: a schedule that starts one chat. Deleting it stops the automation, and it leaves the Scheduled list."}
                </DialogDescription>
              </DialogHeader>
              {removeRefusal && <Notice tone="error">{removeRefusal}</Notice>}
              <DialogFooter>
                <Button type="button" variant="ghost" onClick={leave}>
                  Keep it
                </Button>
                <Button type="submit" variant="danger" disabled={remove.isPending}>
                  {remove.isPending ? "Deleting" : "Delete"}
                </Button>
              </DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
      )}
      {tab === "canvas" || fresh ? (
        <TabPanel idBase={base} value="canvas" className="flex min-h-0 flex-1 flex-col">
          <WorkflowCanvas
            name={saved.name}
            loaded={saved}
            project={project}
            isNew={draft !== null}
            onCreated={() => setCreated(true)}
          />
        </TabPanel>
      ) : (
        <TabPanel idBase={base} value={tab} className="min-h-0 flex-1 overflow-y-auto">
          {tab === "overview" ? (
            <WorkflowOverview project={project} definition={saved} summary={summary} />
          ) : tab === "runs" ? (
            <div className="max-w-4xl">
              <WorkflowRuns project={project} name={saved.name} />
            </div>
          ) : (
            <div className="max-w-4xl" data-testid="workflow-file">
              <CodeBlock language="TOML" code={file} />
            </div>
          )}
        </TabPanel>
      )}
    </div>
  );
}
