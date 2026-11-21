/**
 * The Agents page: the agents the server can run as one list of 36px rows (the name, what it runs on, and at the right
 * a word where something is wrong), and the editor of the one chosen in the editor window (`AgentEditor`,
 * `ui/components/EditorWindow.tsx`). The chosen agent is in the address (`?agent=`, or `?new=1` for a new one), so a
 * reload and a link keep it open; closing the window takes it out of the address. A server without a harness
 * configuration repository keeps its agents in `moebius.toml`: the page then reads the agent from the settings and
 * offers to set the repository up.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useSearchParams } from "react-router";
import { agents as readAgents } from "../../api/calls/chat";
import {
  mcpServers,
  agent as readAgent,
  runners as readRunners,
  skillList,
} from "../../api/calls/harness";
import { availableModels, serverSettings } from "../../api/calls/models";
import { sayError } from "../../api/errorText";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Agent } from "../../api/types/chat";
import type { AgentDetail } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard, SkeletonList } from "../../ui/page/Skeleton";
import { useDrafts } from "../harness/form";
import { HarnessSetup } from "../harness/HarnessSetup";
import { AgentEditor } from "./AgentEditor";
import { type AgentForm, blankForm, copyable, formOf, fromFile } from "./model";
import { AgentWait, NewAgentChoice, NewAgentEditor } from "./NewAgent";

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

/** One agent as a row of 36px that opens it; without `onOpen` (no harness configuration to edit in) it is read only. */
function AgentRow({ agent, onOpen }: { agent: Agent; onOpen?: () => void }) {
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
  return onOpen ? (
    <button
      type="button"
      data-testid="agent-row"
      aria-label={`Edit agent ${agent.name}`}
      onClick={onOpen}
      className={cn(row, "cursor-pointer hover:bg-row-hover")}
    >
      {body}
    </button>
  ) : (
    <div data-testid="agent-row" className={row}>
      {body}
    </div>
  );
}

export function AgentsPage() {
  const [params, setParams] = useSearchParams();
  // What was typed for each agent, held while the page lives, so a window that reopens on it keeps the draft.
  const drafts = useDrafts<AgentForm | null>();
  const newDraft = { hold: drafts.hold(NEW), drop: () => drafts.drop(NEW) };
  const agents = useQuery({ queryKey: keys.agents, queryFn: readAgents });
  const skills = useQuery({ queryKey: keys.skills, queryFn: skillList });
  const mcp = useQuery({ queryKey: keys.mcpServers, queryFn: mcpServers });
  const runners = useQuery({ queryKey: keys.runners, queryFn: readRunners });
  const available = useQuery({
    queryKey: keys.models,
    queryFn: () => availableModels(false, false),
  });

  const chosen = params.get("agent");
  const creating = params.get("new") === "1";
  // A new agent starts blank (`blank=1`) or from an existing agent (`from=`); with neither, the choice is shown.
  const startsFrom = params.get("from");
  const startsBlank = params.get("blank") === "1";
  const select = (changes: Record<string, string | null>) =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        for (const [key, value] of Object.entries(changes)) {
          if (value === null) next.delete(key);
          else next.set(key, value);
        }
        return next;
      },
      { replace: true },
    );
  const open = (name: string) => select({ agent: name, new: null, from: null, blank: null });
  const close = () => select({ agent: null, new: null, from: null, blank: null });

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
  const startNew = () => select({ new: "1", agent: null, from: null, blank: null });
  const shared = {
    inFile,
    skills: skills.isError ? null : (skills.data ?? []),
    mcp: mcp.isError ? null : (mcp.data ?? []),
    runners: runners.data ?? [],
    available: available.data,
    onClose: close,
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title="Agents"
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
          agents.data.map((agent) => (
            <AgentRow
              key={agent.name}
              agent={agent}
              onOpen={editable ? () => open(agent.name) : undefined}
            />
          ))
        )}
      </div>
      {creating && !startsBlank && startsFrom === null ? (
        <NewAgentChoice
          key="choice"
          agents={copyable(agents.data ?? [])}
          onBlank={() => select({ blank: "1" })}
          onFrom={(name) => select({ from: name })}
          onClose={close}
        />
      ) : creating ? (
        <NewAgentEditor
          key={`new-${startsFrom ?? ""}`}
          from={startsBlank ? null : startsFrom}
          agents={agents.data ?? []}
          draft={drafts.take(NEW, () => null)}
          drafts={newDraft}
          {...shared}
        />
      ) : chosen !== null && editable ? (
        <ChosenAgent
          key={chosen}
          name={chosen}
          draft={drafts.take(chosen, () => null)}
          drafts={{ hold: drafts.hold(chosen), drop: () => drafts.drop(chosen) }}
          builtIn={agents.data?.some((agent) => agent.name === chosen && agent.built_in) ?? false}
          {...shared}
        />
      ) : null}
    </div>
  );
}

/** The draft of a new agent, which has no name to hold it yet. */
const NEW = "\u0000new";

type Shared = Omit<React.ComponentProps<typeof AgentEditor>, "initial" | "stored" | "broken">;

/** Reads the files of the agent `name`, and opens the editor on them, or on what was typed for it. */
function ChosenAgent({
  name,
  builtIn,
  draft,
  ...shared
}: Shared & { name: string; builtIn: boolean; draft: AgentForm | null }) {
  const settings = useQuery({
    queryKey: keys.settings,
    queryFn: serverSettings,
    enabled: shared.inFile,
  });
  // The server answers a built-in agent's built-in part without a harness configuration repository.
  const detail = useQuery({
    queryKey: keys.agent(name),
    queryFn: () => readAgent(name),
    enabled: !shared.inFile || builtIn,
    retry: false,
  });

  let opened: AgentDetail | null = null;
  let broken: string | undefined;
  if (shared.inFile) {
    if (settings.isPending || (builtIn && detail.isPending))
      return <Reading name={name} onClose={shared.onClose} />;
    if (settings.isError)
      return <Broken name={name} message={settings.error.message} {...shared} />;
    opened = fromFile(name, settings.data) ?? { name };
    if (detail.data?.built_in) opened = { ...opened, built_in: detail.data.built_in };
  } else {
    if (detail.isPending) return <Reading name={name} onClose={shared.onClose} />;
    if (detail.isError) {
      // The Moebius agent is there without files; saving it makes them.
      if (builtIn && isApiError(detail.error) && detail.error.status === 404) opened = { name };
      else broken = detail.error.message;
    } else {
      opened = detail.data;
    }
  }
  // What a save would replace. The draft is what was typed, which is not the same thing.
  const stored = opened ? formOf(opened) : { ...blankForm(name), exists: true };
  return <AgentEditor {...shared} initial={draft ?? stored} stored={stored} broken={broken} />;
}

function Broken({ name, message, ...shared }: Shared & { name: string; message: string }) {
  const stored = { ...blankForm(name), exists: true };
  return <AgentEditor {...shared} initial={stored} stored={stored} broken={message} />;
}

function Reading({ name, onClose }: { name: string; onClose: () => void }) {
  return (
    <AgentWait title={`Edit agent ${name}`} onClose={onClose}>
      <SkeletonCard lines={4} message="Reading the agent" />
    </AgentWait>
  );
}
