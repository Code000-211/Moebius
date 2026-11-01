/**
 * The Agents page: the agents the server can run and the editor of the one chosen (`AgentEditor`). The chosen agent is
 * in the address (`?agent=`, or `?new=1` for a new one), so a reload and a link keep it. A server without a harness
 * configuration repository keeps its agents in `moebius.toml`: the page then reads the agent from the settings and
 * offers to set the repository up.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
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
import type { TagTone } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { Empty } from "../../ui/page/Empty";
import { chosenOrFirst, ListCard, ListDetail } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { useDrafts } from "../harness/form";
import { HarnessSetup } from "../harness/HarnessSetup";
import { AgentEditor, type Saved } from "./AgentEditor";
import { type AgentForm, blankForm, formOf, fromFile } from "./model";

const harnessName = (harness: string) =>
  harness === "pi-bolt" ? "Pi-Bolt" : harness === "claude-code" ? "Claude Code" : harness;

function cardLine(agent: Agent): string {
  if (agent.error) return agent.error;
  const base = `${agent.model} · ${agent.protocol}`;
  return agent.harness === "claude-code" ? `${base} · ${harnessName(agent.harness)}` : base;
}

function cardTags(agent: Agent): Array<{ label: string; tone: TagTone }> {
  const tags: Array<{ label: string; tone: TagTone }> = [];
  if (agent.built_in) tags.push({ label: "Built in", tone: "plain" });
  if (agent.error) tags.push({ label: "Broken", tone: "error" });
  return tags;
}

export function AgentsPage() {
  const [params, setParams] = useSearchParams();
  const [saved, setSaved] = useState<Saved | null>(null);
  // What was typed for each agent, held while the page lives, so choosing another row keeps the draft.
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

  // A server with a harness configuration repository always lists the built-in skills, so none means agents are kept
  // in the tables of moebius.toml.
  const editable = skills.isSuccess;
  const inFile = skills.isSuccess && skills.data.length === 0;
  const none = agents.isSuccess && agents.data.length === 0;
  // Why the button that adds an agent is quiet, printed beside it so that a keyboard and a screen reader reach it:
  // a `disabled` button is out of the tab order and a tooltip does not go there. When the agents are kept in
  // moebius.toml the box above the list already says what to do, and adding one there is allowed.
  // The first agent is opened rather than an empty pane beside a list that has one in it; a list that cannot be
  // opened yet chooses nothing.
  const picked = chosenOrFirst(
    chosen,
    creating,
    editable ? (agents.data?.map((agent) => agent.name) ?? []) : [],
  );
  const whyQuiet =
    !editable && skills.isError
      ? "The harness configuration could not be read, so no agent can be added yet."
      : undefined;
  const startNew = () => {
    setSaved(null);
    select({ new: "1", agent: null });
  };
  const newForm = blankForm("", true);

  return (
    <>
      <PageHeader
        title="Agents"
        context={
          inFile
            ? "What each run works with: model, instructions, skills and extensions. Saved in moebius.toml."
            : "What each run works with: model, instructions, skills, MCP servers and extensions."
        }
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
      <ListDetail
        list={
          agents.isPending ? (
            <Loader message="Reading the agents" className="py-8" />
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
          ) : (
            agents.data.map((agent) => (
              <ListCard
                key={agent.name}
                title={agent.name}
                tags={cardTags(agent)}
                line={cardLine(agent)}
                alert={agent.error !== undefined}
                selected={editable && !creating && picked === agent.name}
                onClick={
                  editable
                    ? () => {
                        setSaved(null);
                        select({ agent: agent.name, new: null });
                      }
                    : undefined
                }
              />
            ))
          )
        }
        detail={
          creating ? (
            <AgentEditor
              key="new"
              initial={drafts.take(NEW, () => null) ?? newForm}
              stored={newForm}
              drafts={newDraft}
              inFile={inFile}
              skills={skills.isError ? null : (skills.data ?? [])}
              mcp={mcp.isError ? null : (mcp.data ?? [])}
              runners={runners.data ?? []}
              available={available.data}
              saved={saved}
              onSaved={setSaved}
              onNamed={(name) => {
                drafts.drop(NEW);
                select({ agent: name, new: null });
              }}
              onDeleted={() => {
                drafts.drop(NEW);
                select({ agent: null, new: null });
              }}
            />
          ) : picked !== null && editable ? (
            <ChosenAgent
              key={picked}
              name={picked}
              draft={drafts.take(picked, () => null)}
              drafts={{ hold: drafts.hold(picked), drop: () => drafts.drop(picked) }}
              builtIn={
                agents.data?.some((agent) => agent.name === picked && agent.built_in) ?? false
              }
              inFile={inFile}
              skills={skills.isError ? null : (skills.data ?? [])}
              mcp={mcp.isError ? null : (mcp.data ?? [])}
              runners={runners.data ?? []}
              available={available.data}
              saved={saved}
              onSaved={setSaved}
              onNamed={(name) => select({ agent: name, new: null })}
              onDeleted={() => select({ agent: null, new: null })}
            />
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
            <Empty
              title="Nothing chosen"
              body="Pick an agent on the left to change its model, its instructions and the skills it reads."
            />
          )
        }
      />
    </>
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
    if (settings.isPending || (builtIn && detail.isPending)) return <Reading />;
    if (settings.isError)
      return <Broken name={name} message={settings.error.message} {...shared} />;
    opened = fromFile(name, settings.data) ?? { name };
    if (detail.data?.built_in) opened = { ...opened, built_in: detail.data.built_in };
  } else {
    if (detail.isPending) return <Reading />;
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

function Reading() {
  return <p className="m-0 text-base text-secondary">Reading the agent</p>;
}
