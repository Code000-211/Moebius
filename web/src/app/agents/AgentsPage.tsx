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
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Agent } from "../../api/types/chat";
import type { AgentDetail } from "../../api/types/harness";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { Box, ListCard, ListEditor } from "../harness/form";
import { HarnessSetup } from "../harness/HarnessSetup";
import { Header } from "../shell/Page";
import { AgentEditor, type Saved } from "./AgentEditor";
import { blankForm, formOf, fromFile } from "./model";

const harnessName = (harness: string) =>
  harness === "pi-bolt" ? "Pi-Bolt" : harness === "claude-code" ? "Claude Code" : harness;

function cardLine(agent: Agent): string {
  if (agent.error) return agent.error;
  const base = `${agent.model} · ${agent.protocol}`;
  return agent.harness === "claude-code" ? `${base} · ${harnessName(agent.harness)}` : base;
}

export function AgentsPage() {
  const [params, setParams] = useSearchParams();
  const [saved, setSaved] = useState<Saved | null>(null);
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

  return (
    <>
      <div className="flex items-start justify-between gap-base">
        <Header breadcrumb="Build" title="Agents" />
        <Button
          size="sm"
          disabled={!editable}
          onClick={() => {
            setSaved(null);
            select({ new: "1", agent: null });
          }}
        >
          <PlusIcon className="size-icon-sm" weight="bold" />
          New agent
        </Button>
      </div>
      <p className="m-0 mb-plusfifty text-sm text-normal">
        {inFile
          ? "What each run works with: model, instructions, skills, extensions. Saved in moebius.toml."
          : "What each run works with: model, instructions, skills, MCP servers, extensions. An edit is a commit."}
      </p>
      {inFile && (
        <div className="mb-double">
          <HarnessSetup />
        </div>
      )}
      <ListEditor
        list={
          agents.isPending ? (
            <Loader message="Reading the agents…" className="py-double" />
          ) : agents.isError ? (
            <p className="text-sm text-error">{agents.error.message}</p>
          ) : agents.data.length === 0 ? (
            <Box title="Agents">
              <p className="m-0 text-sm text-normal">No agent yet.</p>
            </Box>
          ) : (
            agents.data.map((agent) => (
              <ListCard
                key={agent.name}
                name={agent.name}
                tags={
                  <>
                    {agent.built_in && <Tag>Built in</Tag>}
                    {agent.error && <Tag tone="red">Broken</Tag>}
                  </>
                }
                line={cardLine(agent)}
                red={agent.error !== undefined}
                chosen={editable && !creating && chosen === agent.name}
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
      >
        {creating ? (
          <AgentEditor
            key="new"
            initial={blankForm("", true)}
            inFile={inFile}
            skills={skills.data ?? []}
            mcp={mcp.data ?? []}
            runners={runners.data ?? []}
            available={available.data}
            saved={saved}
            onSaved={setSaved}
            onNamed={(name) => select({ agent: name, new: null })}
            onDeleted={() => select({ agent: null, new: null })}
          />
        ) : chosen !== null && editable ? (
          <ChosenAgent
            key={chosen}
            name={chosen}
            builtIn={agents.data?.some((agent) => agent.name === chosen && agent.built_in) ?? false}
            inFile={inFile}
            skills={skills.data ?? []}
            mcp={mcp.data ?? []}
            runners={runners.data ?? []}
            available={available.data}
            saved={saved}
            onSaved={setSaved}
            onNamed={(name) => select({ agent: name, new: null })}
            onDeleted={() => select({ agent: null, new: null })}
          />
        ) : (
          <p className="m-0 text-sm text-low">Pick an agent to edit it.</p>
        )}
      </ListEditor>
    </>
  );
}

type Shared = Omit<React.ComponentProps<typeof AgentEditor>, "initial" | "broken">;

/** Reads the files of the agent `name`, and opens the editor on them. */
function ChosenAgent({ name, builtIn, ...shared }: Shared & { name: string; builtIn: boolean }) {
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
  return (
    <AgentEditor
      {...shared}
      initial={opened ? formOf(opened) : { ...blankForm(name), exists: true }}
      broken={broken}
    />
  );
}

function Broken({ name, message, ...shared }: Shared & { name: string; message: string }) {
  return (
    <AgentEditor {...shared} initial={{ ...blankForm(name), exists: true }} broken={message} />
  );
}

function Reading() {
  return <p className="m-0 text-sm text-normal">Reading the agent…</p>;
}
