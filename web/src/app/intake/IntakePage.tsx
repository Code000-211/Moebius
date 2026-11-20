/**
 * The Intake page: a project's way in for requests. A project without intake has no form, only (for an owner) the
 * switch that turns it on. With intake, anyone who may see the project, a viewer too, writes a request in their own
 * words; a grilling drafts it into an issue, and a member reads the draft, edits it, accepts it into a card the
 * requester follows, or declines it with a reason the requester is told. A viewer sees only their own requests.
 */
import { TrayArrowDownIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { snapshot } from "../../api/calls/board";
import {
  acceptRequest,
  declineRequest,
  editRequest,
  listRequests,
  setIntake,
  submitRequest,
} from "../../api/calls/intake";
import { listProjects } from "../../api/calls/projects";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { IntakeRequest } from "../../api/types/intake";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { Empty } from "../../ui/page/Empty";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { boardPath } from "../pages";
import { useRole } from "../shell/session";
import { decides, open, ordered, statusWords } from "./model";

export function IntakePage() {
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const [params, setQuery] = useQueryParams();
  if (projects.isPending) {
    return (
      <>
        <PageHeader title="Intake" />
        <SkeletonList rows={3} message="Reading the projects" />
      </>
    );
  }
  if (projects.isError) {
    return (
      <>
        <PageHeader title="Intake" />
        <ReadFailed
          error={projects.error}
          fallback="The projects could not be read. Try again."
          busy={projects.isFetching}
          onRetry={() => void projects.refetch()}
        />
      </>
    );
  }
  const chosen = Number(params.get("project"));
  const project = projects.data.find((each) => each.id === chosen) ?? projects.data[0];
  if (project === undefined) {
    return (
      <>
        <PageHeader title="Intake" />
        <Empty title="No project yet" body="Add a project, and it can take requests." />
      </>
    );
  }
  return (
    <>
      <PageHeader
        title="Intake"
        context="Requests to a project, in the words of whoever asks. Each is turned into a draft issue for a member to accept or decline."
        actions={
          <select
            aria-label="Project"
            className="h-8 rounded-control bg-field px-2 text-base text-text"
            value={project.id}
            onChange={(event) => setQuery({ project: event.target.value })}
          >
            {projects.data.map((each) => (
              <option key={each.id} value={each.id}>
                {each.name}
              </option>
            ))}
          </select>
        }
      />
      <Requests project={project} />
    </>
  );
}

function Requests({ project }: { project: Project }) {
  const role = useRole();
  const queryClient = useQueryClient();
  const turn = useMutation({
    mutationFn: (enabled: boolean) => setIntake(project.id, enabled),
    onSettled: () => void queryClient.invalidateQueries({ queryKey: keys.projects }),
  });
  const requests = useQuery({
    queryKey: keys.requests(project.id),
    queryFn: () => listRequests(project.id),
    enabled: project.intake || decides(role),
  });

  if (!project.intake) {
    return (
      <>
        <Empty
          title={`${project.name} takes no requests`}
          icon={<TrayArrowDownIcon className="size-6 text-secondary" />}
          body="Intake is off for this project, so it has no form."
        />
        {role === "owner" && (
          <div className="mt-4 flex justify-center">
            <Button onClick={() => turn.mutate(true)} disabled={turn.isPending}>
              Turn on intake
            </Button>
          </div>
        )}
        {turn.isError && (
          <Notice tone="error">{sayError(turn.error, "Intake could not be turned on.")}</Notice>
        )}
      </>
    );
  }
  return (
    <div className="flex flex-col gap-6">
      <RequestForm project={project} />
      {requests.isPending ? (
        <SkeletonList rows={2} message="Reading the requests" />
      ) : requests.isError ? (
        <ReadFailed
          error={requests.error}
          fallback="The requests could not be read. Try again."
          busy={requests.isFetching}
          onRetry={() => void requests.refetch()}
        />
      ) : requests.data.length === 0 ? (
        <Empty title="No requests yet" body="What is asked of this project turns up here." />
      ) : (
        <ul className="m-0 flex list-none flex-col gap-3 p-0" data-testid="requests">
          {ordered(requests.data).map((request) => (
            // The card keeps its form state from its first render, so it is made anew when the draft arrives or changes.
            <li key={`${request.id}-${request.status}-${request.title}-${request.body}`}>
              <RequestCard request={request} decider={decides(role)} />
            </li>
          ))}
        </ul>
      )}
      {role === "owner" && (
        <div>
          <Button variant="secondary" onClick={() => turn.mutate(false)} disabled={turn.isPending}>
            Turn off intake
          </Button>
        </div>
      )}
    </div>
  );
}

/** The form: what anyone who may see the project writes, in their own words. */
function RequestForm({ project }: { project: Project }) {
  const [text, setText] = useState("");
  const queryClient = useQueryClient();
  const send = useMutation({
    mutationFn: () => submitRequest(project.id, text),
    onSuccess: () => {
      setText("");
      void queryClient.invalidateQueries({ queryKey: keys.requests(project.id) });
    },
  });
  return (
    <form
      aria-label="Ask for something"
      className="flex flex-col gap-3 rounded-card bg-surface p-4 shadow-card"
      onSubmit={(event) => {
        event.preventDefault();
        if (text.trim() !== "") send.mutate();
      }}
    >
      <Field
        label="What do you need?"
        hint="In your own words. You are told when a member has decided."
      >
        <TextArea rows={4} value={text} onChange={(event) => setText(event.target.value)} />
      </Field>
      {send.isError && (
        <Notice tone="error">{sayError(send.error, "The request could not be sent.")}</Notice>
      )}
      <div>
        <Button type="submit" disabled={send.isPending || text.trim() === ""}>
          Send request
        </Button>
      </div>
    </form>
  );
}

function RequestCard({ request, decider }: { request: IntakeRequest; decider: boolean }) {
  const [title, setTitle] = useState(request.title);
  const [body, setBody] = useState(request.body);
  const [reason, setReason] = useState("");
  const queryClient = useQueryClient();
  const settled = () =>
    void queryClient.invalidateQueries({ queryKey: keys.requests(request.project_id) });
  const save = useMutation({
    mutationFn: () => editRequest(request.id, title, body),
    onSettled: settled,
  });
  const accept = useMutation({
    mutationFn: () => acceptRequest(request.id, { title, body }),
    onSettled: settled,
  });
  const decline = useMutation({
    mutationFn: () => declineRequest(request.id, reason),
    onSettled: settled,
  });
  // The card an accepted request made, found among the boards the page already holds.
  const boards = useQuery({
    queryKey: keys.snapshot,
    queryFn: () => snapshot(),
    enabled: request.card_id !== null,
  });
  const home =
    request.card_id === null
      ? undefined
      : boards.data?.boards.find((b) => b.cards.some((c) => c.id === request.card_id));
  const failed = [save, accept, decline].find((each) => each.isError);
  const editable = decider && open(request);
  return (
    <article
      data-testid="request"
      data-status={request.status}
      className="flex flex-col gap-2 rounded-card bg-surface p-4 shadow-card"
    >
      <p className="m-0 text-sm text-secondary">
        @{request.requester} · {statusWords(request)}
        {home && (
          <>
            {" · "}
            <Link to={`${boardPath(home.repository.id)}?card=${request.card_id}`}>
              Open the card
            </Link>
          </>
        )}
      </p>
      <p className="m-0 whitespace-pre-wrap text-base text-text">{request.text}</p>
      {editable && request.status === "drafted" && (
        <div className="flex flex-col gap-2 border-t border-hairline pt-3">
          <Field label="Draft issue">
            <Input value={title} onChange={(event) => setTitle(event.target.value)} />
          </Field>
          <Field label="Description">
            <TextArea rows={4} value={body} onChange={(event) => setBody(event.target.value)} />
          </Field>
          <div className="flex flex-wrap items-end gap-2">
            <Button
              onClick={() => accept.mutate()}
              disabled={accept.isPending || title.trim() === ""}
            >
              Accept
            </Button>
            <Button variant="secondary" onClick={() => save.mutate()} disabled={save.isPending}>
              Save draft
            </Button>
          </div>
        </div>
      )}
      {editable && (
        <div className="flex flex-wrap items-end gap-2">
          <Field label="Reason for declining" className="min-w-64 flex-1">
            <Input value={reason} onChange={(event) => setReason(event.target.value)} />
          </Field>
          <Button
            variant="secondary"
            onClick={() => decline.mutate()}
            disabled={decline.isPending || reason.trim() === ""}
          >
            Decline
          </Button>
        </div>
      )}
      {failed && <Notice tone="error">{sayError(failed.error, "That did not work.")}</Notice>}
    </article>
  );
}
