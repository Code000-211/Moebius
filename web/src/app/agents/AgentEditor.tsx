/**
 * The editor of one agent, in the editor window (`EditorWindow`, the one the rules open in): the name while a new agent
 * is named, the settings, the instructions (written, or asked of the Moebius agent), what a built-in agent brings
 * itself, and the skills, MCP servers and extensions the agent is given, in one column. Save and Cancel are the
 * window's; Delete (which asks first) is at the left of the foot. Closing with a change asks before it drops it, and
 * what is typed is held with the page (`drafts`) while the window is open. A save is `PUT /v1/agents/{name}`, a commit in
 * the harness configuration repository, and is told by a toast; a server without one keeps its agents in
 * `moebius.toml`, where the same editor saves `agents.<name>` with `PUT /v1/settings` and has paths in place of the
 * repository's skills and MCP servers. A refusal stays in the window, beside the buttons.
 *
 * The instructions box is `AgentInstructions.tsx`, the skills, servers and extensions `AgentFiles.tsx`, and
 * `AgentParts.tsx` the rows and path lists they share.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { deleteAgent, saveAgent } from "../../api/calls/harness";
import { availableModels, saveSettings } from "../../api/calls/models";
import { keys } from "../../api/keys";
import { type Agent, type AvailableModels, listedModel, offeredLevels } from "../../api/types/chat";
import type { AgentDetail, McpServer, Runner, SkillListed } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { EditorWindow } from "../../ui/components/EditorWindow";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { useToast } from "../../ui/page/Toast";
import { useDraft } from "../harness/form";
import { ModelField, ModelNote } from "../harness/ModelField";
import { AgentFiles } from "./AgentFiles";
import { AgentInstructions } from "./AgentInstructions";
import { Item } from "./AgentParts";
import {
  type AgentForm,
  agentRequest,
  ENGINEERING_SKILLS,
  engineeringOf,
  fileTable,
  HARNESSES,
  modelChoices,
  NOT_SET,
  PROTOCOLS,
  THIS_SERVER,
  toggle,
} from "./model";

export interface EditorProps {
  initial: AgentForm;
  /** What the server has for this agent, so Save can say whether anything waits for it even when the editor
   * reopened on a draft, which is not what the file holds. */
  stored: AgentForm;
  /** Where the page keeps what is typed here while the window is open. */
  drafts?: { hold: (form: AgentForm) => void; drop: () => void };
  /** Why the agent's files cannot be read; the agent can then only be deleted. */
  broken?: string;
  inFile: boolean;
  /** The skills `skills/` of the repository offers, or null when the page could not read them. */
  skills: SkillListed[] | null;
  /** The servers `mcp.toml` of the repository holds, or null when the page could not read them. */
  mcp: McpServer[] | null;
  runners: Runner[];
  available: AvailableModels | undefined;
  /** The window closes: after a save or a delete, when the change is dropped or there is none, or (with its draft kept)
   * when the package sheet opens. */
  onClose: () => void;
}

/** What a toast says of a save: the commit it made (none when nothing changed), or that moebius.toml was written. */
function said(commit: string | null | undefined, written: boolean): string {
  if (written) return "Saved in moebius.toml.";
  return commit
    ? `Saved as commit ${commit}.`
    : "Saved. It changed nothing, so there is no commit.";
}

export function AgentEditor(props: EditorProps) {
  const { inFile, skills, mcp, runners, available } = props;
  const queryClient = useQueryClient();
  const toast = useToast();
  const [form, setForm] = useState<AgentForm>(props.initial);
  // What the server has, so that Save says whether it has anything to send even when the editor reopened on a draft.
  const [baseline] = useState<AgentForm>(props.stored);
  const [problem, setProblem] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  // Whether the last ask of the upstreams answered with nothing wrong; it says nothing before the button is pressed.
  const [answered, setAnswered] = useState(false);
  const now = useRef(form);
  now.current = form;
  const dirty = JSON.stringify(form) !== JSON.stringify(baseline);
  // What is typed here stays with the page while the window is open.
  useDraft(form, dirty, props.drafts);
  const change = (patch: Partial<AgentForm>) => {
    setForm((held) => ({ ...held, ...patch }));
    setProblem(null);
  };
  const close = () => {
    props.drafts?.drop();
    props.onClose();
  };

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
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.agents }),
        queryClient.invalidateQueries({ queryKey: keys.agent(form.name.trim()) }),
        queryClient.invalidateQueries({ queryKey: keys.settings }),
        queryClient.invalidateQueries({ queryKey: keys.modelsInUse }),
      ]);
      toast({ kind: "confirmation", text });
      close();
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
    close();
  };
  const choices = modelChoices(available);
  const builtIn = form.builtIn;
  const engineering = engineeringOf(form);
  const levels = [NOT_SET, ...offeredLevels(listedModel(available, form.model))];
  if (form.thinking !== null && !levels.includes(form.thinking)) levels.push(form.thinking);
  const places = [THIS_SERVER, ...runners.map((runner) => runner.name)];
  if (form.runner !== null && !places.includes(form.runner)) places.push(form.runner);
  const broken = props.broken !== undefined;

  return (
    <EditorWindow
      open
      title={form.naming ? "New agent" : `Edit agent ${form.name}`}
      dirty={dirty}
      saving={save.isPending}
      canSave={!broken}
      error={problem}
      actions={
        form.exists && (
          <Button variant="danger" size="sm" onClick={() => setDeleting(true)}>
            Delete agent
          </Button>
        )
      }
      onSave={() => save.mutate()}
      onClose={close}
    >
      {broken ? (
        <>
          <Notice tone="error">{props.broken}</Notice>
          <p className="m-0 text-base text-secondary">
            Its files cannot be read, so it cannot run. Fix them in the repository or delete the
            agent.
          </p>
        </>
      ) : (
        <>
          {form.naming && (
            <Field label="Name of the new agent" hint="Letters, digits, - and _.">
              {(id) => (
                <Input
                  id={id}
                  mono
                  autoFocus
                  value={form.name}
                  placeholder="Name of the new agent"
                  onChange={(event) => change({ name: event.target.value })}
                />
              )}
            </Field>
          )}
          <Field label="Model">
            {(id) => (
              <ModelField
                id={id}
                choices={choices}
                value={form.model}
                placeholder="Model name"
                onChange={(model) => change({ model })}
                onPick={(choice) =>
                  change({
                    model: choice.id,
                    ...((PROTOCOLS as readonly string[]).includes(choice.protocol)
                      ? { protocol: choice.protocol }
                      : {}),
                  })
                }
              />
            )}
          </Field>
          <div className="grid grid-cols-2 gap-4">
            <Field label="Protocol">
              {(id) => (
                <Select
                  id={id}
                  options={[...PROTOCOLS]}
                  value={form.protocol}
                  onChange={(protocol) => change({ protocol })}
                />
              )}
            </Field>
            <Field label="Harness">
              {(id) => (
                <Select
                  id={id}
                  options={[...HARNESSES]}
                  value={form.harness}
                  onChange={(harness) => change({ harness })}
                />
              )}
            </Field>
            {(runners.length > 0 || form.runner !== null) && (
              <Field label="Runner">
                {(id) => (
                  <Select
                    id={id}
                    options={places}
                    value={form.runner ?? THIS_SERVER}
                    onChange={(runner) =>
                      change({ runner: runner === THIS_SERVER ? null : runner })
                    }
                  />
                )}
              </Field>
            )}
            <Field label="Thinking">
              {(id) => (
                <Select
                  id={id}
                  options={levels}
                  value={form.thinking ?? NOT_SET}
                  onChange={(level) => change({ thinking: level === NOT_SET ? null : level })}
                />
              )}
            </Field>
          </div>
          <Field label="Token budget per card">
            {(id) => (
              <Input
                id={id}
                value={form.budget}
                placeholder="No limit"
                className="tabular-nums"
                onChange={(event) => change({ budget: event.target.value })}
              />
            )}
          </Field>
          <ModelNote
            available={available}
            onRefresh={() => void refresh()}
            refreshing={refreshing}
            answered={answered}
          />
          <AgentInstructions form={form} onChange={change} onProblem={setProblem} />
          {builtIn && (
            <>
              <Box
                title="Built-in instructions"
                aside="Read only"
                className="flex flex-col gap-4 [&>header]:mb-0"
              >
                <pre
                  data-testid="built-in-instructions"
                  className="m-0 overflow-x-auto whitespace-pre-wrap rounded-control bg-code p-4 font-mono text-sm text-text"
                >
                  {builtIn.instructions}
                </pre>
              </Box>
              <Box
                title="Built-in skills"
                aside="Read only"
                className="flex flex-col gap-3 [&>header]:mb-0"
              >
                {builtIn.skills.length === 0 && (
                  <p className="m-0 text-base text-secondary">No built-in skill.</p>
                )}
                {builtIn.skills.map((skill) => (
                  <Item
                    key={skill.name}
                    name={<span className="font-mono text-sm text-text">{skill.name}</span>}
                    line={skill.description}
                  />
                ))}
              </Box>
            </>
          )}
          <Box
            title="Engineering skills"
            aside={`${engineering.length} of ${ENGINEERING_SKILLS.length}`}
            className="flex flex-col gap-3 [&>header]:mb-0"
          >
            {ENGINEERING_SKILLS.map((skill) => (
              <Item
                key={skill.name}
                name={
                  <div>
                    <Chip
                      selected={engineering.includes(skill.name)}
                      onClick={() => change({ engineeringSkills: toggle(engineering, skill.name) })}
                    >
                      {skill.name}
                    </Chip>
                  </div>
                }
                line={skill.line}
              />
            ))}
          </Box>
          <AgentFiles
            form={form}
            change={change}
            inFile={inFile}
            skills={skills}
            mcp={mcp}
            onBrowse={props.onClose}
          />
        </>
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
    </EditorWindow>
  );
}
