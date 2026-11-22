/**
 * The Agents page: the agents the server can run as one list of 36px rows (the name, what it runs on, and at the right
 * a word where something is wrong). A row opens the agent's own page (`AgentPage.tsx`, `/agents/<name>`). "New agent"
 * asks one plain question, blank or from an agent that exists, and opens the page of a new agent
 * (`/agents/new?blank=1`, `?from=`), which is named and saved there. `?new=1` shows the question, so a link keeps it
 * open; the old addresses `?agent=` and `?new=1&blank=1` lead to the pages that replaced them (`routes/redirects.ts`).
 * A server without a harness configuration repository keeps its agents in `moebius.toml`: the page then offers to set
 * the repository up.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { Link, Navigate, useLocation, useNavigate, useSearchParams } from "react-router";
import { agents as readAgents } from "../../api/calls/chat";
import { skillList } from "../../api/calls/harness";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Agent } from "../../api/types/chat";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonList } from "../../ui/page/Skeleton";
import { HarnessSetup } from "../harness/HarnessSetup";
import { agentPath, NEW_AGENT_PATH } from "../pages";
import { oldAgentAddress } from "../routes/redirects";
import { AgentsTabs } from "./AgentsTabs";
import { copyable } from "./model";
import { NewAgentChoice } from "./NewAgent";

const harnessName = (harness: string) =>
  harness === "pi-bolt" ? "Pi-Bolt" : harness === "claude-code" ? "Claude Code" : harness;

function rowLine(agent: Agent): string {
  if (agent.error) return agent.error;
  if (agent.model === "") return "Needs a model";
  const base = `${agent.model} · ${agent.protocol}`;
  return agent.harness === "claude-code" ? `${base} · ${harnessName(agent.harness)}` : base;
}

/** The one word at the right of a row: that the agent is broken, or built in. */
function rowState(agent: Agent): { text: string; className: string } | null {
  if (agent.error) return { text: "Broken", className: "text-error-text" };
  if (agent.built_in) return { text: "Built in", className: "text-muted" };
  return null;
}

/** One agent as a row of 36px that opens its page; without `open` (no harness configuration to edit in) it is read only. */
function AgentRow({ agent, open }: { agent: Agent; open: boolean }) {
  const state = rowState(agent);
  const body = (
    <>
      <span className="shrink-0 truncate font-medium" style={{ maxWidth: "14rem" }}>
        {agent.name}
      </span>
      <span
        className={cn(
          "min-w-0 flex-1 truncate",
          agent.error
            ? "text-error-text"
            : agent.model === ""
              ? "text-attention-text"
              : "text-muted",
        )}
      >
        {rowLine(agent)}
      </span>
      {state && (
        <span className={cn("ml-auto shrink-0 text-xs", state.className)}>{state.text}</span>
      )}
    </>
  );
  const row =
    "flex h-9 w-full items-center gap-2.5 border-b border-hairline px-4 text-left text-base text-text";
  return open ? (
    <Link
      to={agentPath(agent.name)}
      data-testid="agent-row"
      aria-label={`Open agent ${agent.name}`}
      className={cn(row, "cursor-pointer hover:bg-row-hover")}
    >
      {body}
    </Link>
  ) : (
    <div data-testid="agent-row" className={row}>
      {body}
    </div>
  );
}

export function AgentsPage() {
  const { search } = useLocation();
  const [params, setParams] = useSearchParams();
  const navigate = useNavigate();
  const agents = useQuery({ queryKey: keys.agents, queryFn: readAgents });
  const skills = useQuery({ queryKey: keys.skills, queryFn: skillList });
  const old = oldAgentAddress(new URLSearchParams(search));
  const choosing = params.get("new") === "1";
  const setChoosing = (on: boolean) => setParams(on ? { new: "1" } : {}, { replace: true });

  // A server with a harness configuration repository always lists the built-in skills, so none means agents are kept
  // in the tables of moebius.toml.
  const editable = skills.isSuccess;
  const inFile = skills.isSuccess && skills.data.length === 0;
  const none = agents.isSuccess && agents.data.length === 0;
  // Why the button that adds an agent is quiet, printed beside it so that a keyboard and a screen reader reach it:
  // a `disabled` button is out of the tab order and a tooltip does not go there. When the agents are kept in
  // moebius.toml the box above the list already says what to do, and adding one there is allowed.
  const whyQuiet =
    !editable && skills.isError
      ? "The harness configuration could not be read, so no agent can be added yet."
      : undefined;
  const startNew = () => setChoosing(true);
  if (old !== null) return <Navigate to={old} replace />;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title="Agents"
        tabs={<AgentsTabs value="agents" />}
        actions={
          <div className="flex flex-wrap items-center gap-3">
            {whyQuiet && <span className="text-sm text-secondary">{whyQuiet}</span>}
            <Button size="sm" disabled={!editable} onClick={startNew}>
              <PlusIcon className="size-4" weight="bold" />
              New agent
            </Button>
          </div>
        }
      />
      {inFile && (
        <div className="mb-8">
          <HarnessSetup />
        </div>
      )}
      {skills.isError && (
        <div className="mb-8">
          <Notice
            tone="error"
            action={
              <Button
                size="sm"
                variant="secondary"
                disabled={skills.isFetching}
                onClick={() => void skills.refetch()}
              >
                Try again
              </Button>
            }
          >
            {`Could not read the harness configuration, so no agent can be edited or added. ${skills.error.message}`}
          </Notice>
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {agents.isPending ? (
          <SkeletonList rows={4} message="Reading the agents" />
        ) : agents.isError ? (
          <Notice
            tone="error"
            action={
              <Button
                size="sm"
                variant="secondary"
                disabled={agents.isFetching}
                onClick={() => void agents.refetch()}
              >
                Try again
              </Button>
            }
          >
            {sayError(agents.error, "Could not read the agents. Try again.")}
          </Notice>
        ) : none ? (
          <Empty
            title="No agents yet"
            body="An agent is what a run works with: its model, its instructions, the skills it reads. Create one to give the coder something to start from."
            action={
              editable ? (
                <Button size="sm" onClick={startNew}>
                  <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
                  New agent
                </Button>
              ) : undefined
            }
          />
        ) : (
          agents.data.map((agent) => <AgentRow key={agent.name} agent={agent} open={editable} />)
        )}
      </div>
      {choosing && (
        <NewAgentChoice
          agents={copyable(agents.data ?? [])}
          onBlank={() => void navigate(`${NEW_AGENT_PATH}?blank=1`)}
          onFrom={(name) => void navigate(`${NEW_AGENT_PATH}?from=${encodeURIComponent(name)}`)}
          onClose={() => setChoosing(false)}
        />
      )}
    </div>
  );
}
