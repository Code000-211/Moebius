/**
 * The page of one agent, `/agents/<name>` (`/agents/new` while a new agent is named): the head has the way back to
 * Agents, the agent's name, the tabs Settings | Instructions | Extensions (`?tab=`, with the count of what the agent is
 * given), Delete (which asks first) and Save, which is enabled while something has changed. Leaving with a change asks
 * first (`LeaveGuard`, the one the wiki editor uses). A save is `PUT /v1/agents/{name}`, a commit in the harness
 * configuration repository, and is told by a toast; a server without one keeps its agents in `moebius.toml`, where the
 * same page saves `agents.<name>` with `PUT /v1/settings` and has paths in place of the repository's skills and MCP
 * servers. A new agent is saved under the name typed and its page opens. A refusal is said under the head.
 *
 * The tabs are `AgentSettings.tsx`, `AgentInstructions.tsx` and `AgentExtensions.tsx`.
 */
import { CaretRightIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { deleteAgent, saveAgent } from "../../api/calls/harness";
import { availableModels, saveSettings } from "../../api/calls/models";
import { installedPi } from "../../api/calls/packages";
import { keys } from "../../api/keys";
import type { Agent, AvailableModels } from "../../api/types/chat";
import type { AgentDetail, McpServer, Runner, SkillListed } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { useToast } from "../../ui/page/Toast";
import { HarnessSetup } from "../harness/HarnessSetup";
import { agentPath } from "../pages";
import { LeaveGuard } from "../wiki/LeaveGuard";
import { AgentExtensions, packagesOf } from "./AgentExtensions";
import { AgentInstructions } from "./AgentInstructions";
import { AgentSettings } from "./AgentSettings";
import { type AgentForm, agentRequest, fileTable } from "./model";

export const AGENT_TABS = ["settings", "instructions", "extensions"] as const;
export type AgentTab = (typeof AGENT_TABS)[number];

export interface AgentViewProps {
  initial: AgentForm;
  /** What the server has for this agent, so Save says whether anything waits for it. */
  stored: AgentForm;
  /** Why the agent's files cannot be read; the agent can then only be deleted. */
  broken?: string;
  inFile: boolean;
  /** The skills `skills/` of the repository offers, or null when the page could not read them. */
  skills: SkillListed[] | null;
  /** The servers `mcp.toml` of the repository holds, or null when the page could not read them. */
  mcp: McpServer[] | null;
  runners: Runner[];
  available: AvailableModels | undefined;
}

/** What a toast says of a save: the commit it made (none when nothing changed), or that moebius.toml was written. */
function said(commit: string | null | undefined, written: boolean): string {
  if (written) return "Saved in moebius.toml.";
  return commit
    ? `Saved as commit ${commit}.`
    : "Saved. It changed nothing, so there is no commit.";
}

/** The way back to the list, in the place a page reached by drilling down carries it: before the name. */
export function AgentsCrumb() {
  return (
    <Link
      to="/agents"
      className="inline-flex items-center gap-1 rounded-control text-md text-muted hover:text-text"
    >
      Agents
      <CaretRightIcon aria-hidden="true" className="size-3" weight="bold" />
    </Link>
  );
}

export function AgentView(props: AgentViewProps) {
  const { inFile, skills, mcp, runners, available } = props;
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const toast = useToast();
  const ids = useId();
  const [params, setParams] = useSearchParams();
  const asked = params.get("tab");
  const tab: AgentTab = (AGENT_TABS as readonly string[]).includes(asked ?? "")
    ? (asked as AgentTab)
    : "settings";
  const [form, setForm] = useState<AgentForm>(props.initial);
  // What the server has, so that Save says whether it has anything to send; a save moves it.
  const [baseline, setBaseline] = useState<AgentForm>(props.stored);
  const [problem, setProblem] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  // Whether the last ask of the upstreams answered with nothing wrong; it says nothing before the button is pressed.
  const [answered, setAnswered] = useState(false);
  const dirty = JSON.stringify(form) !== JSON.stringify(baseline);
  // A save or a delete that moves to another address is not leaving with a change.
  const leaving = useRef(false);
  const changed = useRef(dirty);
  changed.current = dirty;
  const change = (patch: Partial<AgentForm>) => {
    setForm((held) => ({ ...held, ...patch }));
    setProblem(null);
  };
  const choose = (next: AgentTab) =>
    setParams(
      (current) => {
        const copy = new URLSearchParams(current);
        copy.set("tab", next);
        return copy;
      },
      { replace: true },
    );

  const refresh = async () => {
    setRefreshing(true);
    setAnswered(false);
    try {
      const fresh = await availableModels(true, false);
      queryClient.setQueryData(keys.models, fresh);
      setAnswered(fresh.errors.length === 0);
    } finally {
      setRefreshing(false);
    }
  };

  const packages = useQuery({ queryKey: keys.piInstalled, queryFn: installedPi, retry: false });
  const given =
    (packages.data ? packagesOf(packages.data, form.name).length : 0) + form.extensions.length;

  const save = useMutation({
    mutationFn: async (): Promise<string> => {
      const request = agentRequest(form);
      if (typeof request === "string") throw new Error(request);
      const { name, settings, instructions } = request;
      // A new agent never replaces one that exists; the Coder and the Reviewer always do.
      if (form.naming) {
        const listed = queryClient.getQueryData<Agent[]>(keys.agents) ?? [];
        if (listed.some((agent) => agent.name === name)) {
          throw new Error(`An agent named ${name} exists. Give the new agent another name.`);
        }
      }
      if (inFile) {
        await saveSettings({ [`agents.${name}`]: fileTable(settings, instructions) }, []);
        return said(null, true);
      }
      const detail: AgentDetail = await saveAgent(name, settings, instructions);
      return said(detail.commit, false);
    },
    onSuccess: async (text) => {
      const name = form.name.trim();
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.agents }),
        queryClient.invalidateQueries({ queryKey: keys.agent(name) }),
        queryClient.invalidateQueries({ queryKey: keys.settings }),
        queryClient.invalidateQueries({ queryKey: keys.modelsInUse }),
      ]);
      toast({ kind: "confirmation", text });
      if (form.naming) {
        // The agent now exists: its own page opens, read from the server.
        leaving.current = true;
        void navigate(agentPath(name, tab), { replace: true });
      } else {
        setBaseline(form);
      }
    },
    onError: (error) => setProblem(error.message),
  });
  const remove = async () => {
    if (inFile) await saveSettings({}, [`agents.${form.name}`]);
    else await deleteAgent(form.name);
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: keys.agents }),
      queryClient.invalidateQueries({ queryKey: keys.settings }),
    ]);
    leaving.current = true;
    void navigate("/agents", { replace: true });
  };
  const broken = props.broken !== undefined;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <LeaveGuard
        dirty={() => changed.current && !leaving.current}
        samePage
        what="What you changed on this agent is not saved yet. Leaving now loses it."
      />
      <PageHeader
        oneRow
        back={<AgentsCrumb />}
        title={form.naming ? "New agent" : form.name}
        tabs={
          broken ? undefined : (
            <Tabs
              tabs={[
                { value: "settings", label: "Settings" },
                { value: "instructions", label: "Instructions" },
                { value: "extensions", label: "Extensions", count: given },
              ]}
              value={tab}
              onChange={choose}
              label="Agent"
              idBase={ids}
            />
          )
        }
        actions={
          form.exists && (
            <Button variant="danger" size="sm" onClick={() => setDeleting(true)}>
              Delete
            </Button>
          )
        }
        primary={
          !broken && (
            <Button size="sm" disabled={save.isPending || !dirty} onClick={() => save.mutate()}>
              {save.isPending ? "Saving" : "Save"}
            </Button>
          )
        }
      />
      {inFile && (
        <div className="mb-6">
          <HarnessSetup />
        </div>
      )}
      {problem && (
        <p role="alert" className="mb-4 mt-0 break-words text-sm text-error-text">
          {problem}
        </p>
      )}
      {broken ? (
        <div className="flex flex-col gap-3">
          <Notice tone="error">{props.broken}</Notice>
          <p className="m-0 text-base text-secondary">
            Its files cannot be read, so it cannot run. Fix them in the repository or delete the
            agent.
          </p>
        </div>
      ) : (
        <TabPanel
          idBase={ids}
          value={tab}
          className={
            tab === "settings" ? "min-h-0 flex-1 overflow-y-auto" : "flex min-h-0 flex-1 flex-col"
          }
        >
          {tab === "settings" && (
            <AgentSettings
              form={form}
              change={change}
              inFile={inFile}
              skills={skills}
              mcp={mcp}
              runners={runners}
              available={available}
              builtIn={form.builtIn}
              refresh={{ run: () => void refresh(), busy: refreshing, answered }}
            />
          )}
          {tab === "instructions" && (
            <AgentInstructions
              form={form}
              builtIn={form.builtIn}
              onChange={change}
              onProblem={setProblem}
            />
          )}
          {tab === "extensions" && (
            <div className="min-h-0 flex-1 overflow-y-auto">
              <AgentExtensions form={form} change={change} />
            </div>
          )}
        </TabPanel>
      )}
      <ConfirmDelete
        open={deleting}
        onOpenChange={setDeleting}
        title="Delete this agent?"
        confirmLabel="Delete agent"
        onConfirm={remove}
      >
        The agent {form.name} and its files are removed.
      </ConfirmDelete>
    </div>
  );
}
