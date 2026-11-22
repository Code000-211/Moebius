/**
 * The route of one agent's page (`/agents/<name>`, or `/agents/new` with `?blank=1` or `?from=<agent>`): reads the agent
 * (its files from the harness configuration repository, or its table of `moebius.toml` on a server without one), and what
 * the page's tabs offer (skills, MCP servers, runners, the models the upstreams serve), and opens `AgentView` on them.
 * While the agent is read the page has its head and a skeleton; an agent whose files cannot be read has the reason and
 * Delete only.
 */
import { useQuery } from "@tanstack/react-query";
import { useParams, useSearchParams } from "react-router";
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
import type { AgentDetail } from "../../api/types/harness";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { AgentsCrumb, AgentView, type AgentViewProps } from "./AgentView";
import { blankForm, copyOf, formOf, fromFile } from "./model";

type Shared = Omit<AgentViewProps, "initial" | "stored" | "broken">;

export function AgentPage({ creating = false }: { creating?: boolean }) {
  const { name = "" } = useParams();
  const [params] = useSearchParams();
  const agents = useQuery({ queryKey: keys.agents, queryFn: readAgents });
  const skills = useQuery({ queryKey: keys.skills, queryFn: skillList });
  const mcp = useQuery({ queryKey: keys.mcpServers, queryFn: mcpServers });
  const runners = useQuery({ queryKey: keys.runners, queryFn: readRunners });
  const available = useQuery({
    queryKey: keys.models,
    queryFn: () => availableModels(false, false),
  });
  // A server with a harness configuration repository always lists the built-in skills, so none means agents are kept
  // in the tables of moebius.toml.
  const inFile = skills.isSuccess && skills.data.length === 0;
  const shared: Shared = {
    inFile,
    skills: skills.isError ? null : (skills.data ?? []),
    mcp: mcp.isError ? null : (mcp.data ?? []),
    runners: runners.data ?? [],
    available: available.data,
  };
  if (skills.isPending) return <Reading title={creating ? "New agent" : name} />;
  if (skills.isError) {
    return (
      <Failed title={creating ? "New agent" : name}>
        {`Could not read the harness configuration, so the agent cannot be edited. ${skills.error.message}`}
      </Failed>
    );
  }
  if (creating) {
    const from = params.get("blank") === "1" ? null : params.get("from");
    return <NewAgent from={from} {...shared} agents={agents.data ?? []} />;
  }
  const builtIn = agents.data?.some((agent) => agent.name === name && agent.built_in) ?? false;
  return <ChosenAgent key={name} name={name} builtIn={builtIn} {...shared} />;
}

function NewAgent({
  from,
  agents,
  ...shared
}: Shared & { from: string | null; agents: Parameters<typeof copyOf>[1][] }) {
  const blank = blankForm("", true);
  const detail = useQuery({
    queryKey: keys.agent(from ?? ""),
    queryFn: () => readAgent(from as string),
    enabled: from !== null,
    retry: false,
  });
  if (from === null) return <AgentView key="blank" {...shared} initial={blank} stored={blank} />;
  if (detail.isPending) return <Reading title="New agent" />;
  if (detail.isError) {
    return <Failed title="New agent">{`Could not read ${from}. ${detail.error.message}`}</Failed>;
  }
  const copy = copyOf(
    detail.data,
    agents.find((agent) => agent?.name === from),
  );
  // What a save would replace is nothing: the copy is waiting to be saved from the start.
  return <AgentView key={`from-${from}`} {...shared} initial={copy} stored={blank} />;
}

/** Reads the files of the agent `name`, and opens its page on them. */
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
    if (settings.isPending || (builtIn && detail.isPending)) return <Reading title={name} />;
    if (settings.isError) broken = settings.error.message;
    else {
      opened = fromFile(name, settings.data) ?? { name };
      if (detail.data?.built_in) opened = { ...opened, built_in: detail.data.built_in };
    }
  } else {
    if (detail.isPending) return <Reading title={name} />;
    if (detail.isError) {
      // The Moebius agent is there without files; saving it makes them.
      if (builtIn && isApiError(detail.error) && detail.error.status === 404) opened = { name };
      else broken = detail.error.message;
    } else {
      opened = detail.data;
    }
  }
  // What a save would replace.
  const stored = opened ? formOf(opened) : { ...blankForm(name), exists: true };
  return <AgentView {...shared} initial={stored} stored={stored} broken={broken} />;
}

function Reading({ title }: { title: string }) {
  return (
    <>
      <PageHeader oneRow back={<AgentsCrumb />} title={title} />
      <SkeletonCard lines={4} message="Reading the agent" />
    </>
  );
}

function Failed({ title, children }: { title: string; children: string }) {
  return (
    <>
      <PageHeader oneRow back={<AgentsCrumb />} title={title} />
      <Notice tone="error">{children}</Notice>
    </>
  );
}
