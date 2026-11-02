/**
 * The editor of one agent (the desktop app's A1 Bento workbench): the head with Save and Delete, the settings row, the
 * instructions (written, or asked of the Moebius agent), what a built-in agent brings itself, and the skills, MCP
 * servers and extensions the agent is given. A save is `PUT /v1/agents/{name}`, a commit in the harness configuration
 * repository; a server without one keeps its agents in `moebius.toml`, where the same editor saves
 * `agents.<name>` with `PUT /v1/settings` and has paths in place of the repository's skills and MCP servers.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { Link } from "react-router";
import { startChat } from "../../api/calls/chat";
import { deleteAgent, saveAgent } from "../../api/calls/harness";
import { availableModels, saveSettings } from "../../api/calls/models";
import { keys } from "../../api/keys";
import type { AvailableModels } from "../../api/types/chat";
import type { AgentDetail, McpServer, Runner, SkillListed } from "../../api/types/harness";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Field, Select, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { ActionRow, Committed, EditorHead, useDraft } from "../harness/form";
import { instructionsRequest } from "../harness/importRequest";
import { ModelField, ModelNote } from "../harness/ModelField";
import {
  type AgentForm,
  addPath,
  agentRequest,
  ENGINEERING_AGENTS,
  fileTable,
  HARNESSES,
  modelChoices,
  NOT_SET,
  PROTOCOLS,
  THIS_SERVER,
  toggle,
  withChosen,
} from "./model";

const LEVELS = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const COMPRESS = ["Compressed", "As they are"];
const ENGINEERING = ["Given", "Not given"];

/** What the page says of the last save: the commit it made (none when nothing changed), or that moebius.toml was written. */
export type Saved = { name: string; commit: string | null } | { name: string; written: true };

export interface EditorProps {
  initial: AgentForm;
  /** What the server has for this agent, so Save can say whether anything waits for it even when the editor
   * reopened on a draft, which is not what the file holds. */
  stored: AgentForm;
  /** Where the page keeps what is typed here, so choosing another row does not lose it. */
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
  saved: Saved | null;
  onSaved: (saved: Saved | null) => void;
  /** The agent is saved (under this name) or deleted. */
  onNamed: (name: string) => void;
  onDeleted: () => void;
}

/** A row of a list: what names it, and a line under it that is red when something is wrong. */
function Item({ name, line, red }: { name: React.ReactNode; line?: string; red?: boolean }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      {name}
      {line && (
        <span
          className={`min-w-0 break-words text-sm ${red ? "text-error-text" : "text-secondary"}`}
        >
          {line}
        </span>
      )}
    </div>
  );
}

function Paths({
  heading,
  aside,
  empty,
  placeholder,
  addLabel,
  paths,
  onChange,
}: {
  heading: string;
  aside: string;
  empty: string;
  placeholder: string;
  addLabel: string;
  paths: string[];
  onChange: (paths: string[]) => void;
}) {
  const [typed, setTyped] = useState("");
  const add = () => {
    onChange(addPath(paths, typed));
    setTyped("");
  };
  return (
    <Box title={heading} aside={aside} className="flex flex-col gap-4">
      {paths.length === 0 && <p className="m-0 text-base text-secondary">{empty}</p>}
      {paths.map((path) => (
        <div key={path} className="flex min-h-10 flex-wrap items-center justify-between gap-3">
          <span className="min-w-0 flex-1 break-all font-mono text-sm text-text">{path}</span>
          <Button
            variant="danger"
            size="sm"
            aria-label={`Remove ${path}`}
            onClick={() => onChange(paths.filter((held) => held !== path))}
          >
            Remove
          </Button>
        </div>
      ))}
      <div className="flex flex-wrap items-end gap-3">
        <Input
          mono
          className="min-w-60 flex-1"
          aria-label={`${addLabel.replace("Add ", "")} path`}
          value={typed}
          placeholder={placeholder}
          onChange={(event) => setTyped(event.target.value)}
          onCommandEnter={add}
        />
        <Button
          variant="secondary"
          size="sm"
          disabled={typed.trim() === ""}
          title={typed.trim() === "" ? "Type the path first" : undefined}
          onClick={add}
        >
          {addLabel}
        </Button>
      </div>
    </Box>
  );
}

export function AgentEditor(props: EditorProps) {
  const { inFile, skills, mcp, runners, available, saved, onSaved } = props;
  const queryClient = useQueryClient();
  const [form, setForm] = useState<AgentForm>(props.initial);
  // What the server has, so that Save says whether it has anything to send even when the editor reopened on a draft.
  const [baseline, setBaseline] = useState<AgentForm>(props.stored);
  const [problem, setProblem] = useState<string | null>(null);
  const [byAgent, setByAgent] = useState(false);
  const [wish, setWish] = useState("");
  const [refreshing, setRefreshing] = useState(false);
  // Whether the last ask of the upstreams answered with nothing wrong; it says nothing before the button is pressed.
  const [answered, setAnswered] = useState(false);
  const now = useRef(form);
  now.current = form;
  const dirty = JSON.stringify(form) !== JSON.stringify(baseline);
  // What is typed here stays with the page, so moving along the list and back does not lose it.
  useDraft(form, dirty, props.drafts);
  const change = (patch: Partial<AgentForm>) => {
    setForm((held) => ({ ...held, ...patch }));
    setProblem(null);
    onSaved(null);
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
    mutationFn: async (): Promise<Saved> => {
      const request = agentRequest(form);
      if (typeof request === "string") throw new Error(request);
      const { name, settings, instructions } = request;
      if (inFile) {
        await saveSettings({ [`agents.${name}`]: fileTable(settings, instructions) }, []);
        return { name, written: true };
      }
      const detail: AgentDetail = await saveAgent(name, settings, instructions);
      return { name, commit: detail.commit ?? null };
    },
    onSuccess: async (result) => {
      const next = { ...now.current, name: result.name, naming: false, exists: true };
      setForm(next);
      setBaseline(next);
      setProblem(null);
      onSaved(result);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.agents }),
        queryClient.invalidateQueries({ queryKey: keys.agent(result.name) }),
        queryClient.invalidateQueries({ queryKey: keys.settings }),
        queryClient.invalidateQueries({ queryKey: keys.modelsInUse }),
      ]);
      props.onNamed(result.name);
    },
    onError: (error) => setProblem(error.message),
  });
  const remove = useMutation({
    mutationFn: async () => {
      if (inFile) await saveSettings({}, [`agents.${form.name}`]);
      else await deleteAgent(form.name);
    },
    onSuccess: async () => {
      props.drafts?.drop();
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.agents }),
        queryClient.invalidateQueries({ queryKey: keys.settings }),
      ]);
      props.onDeleted();
    },
    onError: (error) => setProblem(error.message),
  });
  const ask = useMutation({
    mutationFn: () =>
      startChat({ agent: "Moebius", message: instructionsRequest(form.name, wish.trim()) }),
    onError: (error) => setProblem(error.message),
  });

  const choices = modelChoices(available);
  const builtIn = form.builtIn;
  const skillsOffered = (skills ?? []).filter((skill) => !skill.built_in);
  const levels = [NOT_SET, ...LEVELS];
  if (form.thinking !== null && !levels.includes(form.thinking)) levels.push(form.thinking);
  const places = [THIS_SERVER, ...runners.map((runner) => runner.name)];
  if (form.runner !== null && !places.includes(form.runner)) places.push(form.runner);
  const showSaved = saved && saved.name === form.name;

  const head = (
    <Box className="flex flex-col gap-4">
      <EditorHead
        name={form.name}
        naming={form.naming}
        exists={form.exists}
        deleteLabel="Delete agent"
        namePlaceholder="Name of the new agent"
        onName={(name) => change({ name })}
        saving={save.isPending}
        canSave={props.broken === undefined}
        dirty={dirty}
        saveHint={props.broken === undefined ? undefined : "Its files cannot be read"}
        onSave={() => save.mutate()}
        onDelete={() => remove.mutate()}
        deleting={remove.isPending}
        readOnly={
          props.broken === undefined ? undefined : (
            <ActionRow
              exists
              deleteLabel="Delete agent"
              name={form.name}
              saving={false}
              canSave={false}
              saveHint="Its files cannot be read"
              onSave={() => {}}
              onDelete={() => remove.mutate()}
              deleting={remove.isPending}
            />
          )
        }
      />
      {props.broken !== undefined ? (
        <>
          <Notice tone="error">{props.broken}</Notice>
          <p className="m-0 max-w-prose text-base text-secondary">
            Its files cannot be read, so it cannot run. Fix them in the repository or delete the
            agent.
          </p>
        </>
      ) : (
        <>
          <div className="grid items-start gap-4 sm:grid-cols-2 xl:grid-cols-3">
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
            {form.harness === HARNESSES[0] && (
              <>
                <Field label="Hide tool outputs after turns">
                  {(id) => (
                    <Input
                      id={id}
                      value={form.hideAfter}
                      placeholder="0 keeps them"
                      className="tabular-nums"
                      onChange={(event) => change({ hideAfter: event.target.value })}
                    />
                  )}
                </Field>
                <Field label="Lines per tool result">
                  {(id) => (
                    <Input
                      id={id}
                      value={form.maxLines}
                      placeholder="0 is no limit"
                      className="tabular-nums"
                      onChange={(event) => change({ maxLines: event.target.value })}
                    />
                  )}
                </Field>
                <Field label="Bytes per tool result">
                  {(id) => (
                    <Input
                      id={id}
                      value={form.maxBytes}
                      placeholder="0 is no limit"
                      onChange={(event) => change({ maxBytes: event.target.value })}
                    />
                  )}
                </Field>
                <Field label="Tool outputs">
                  {(id) => (
                    <Select
                      id={id}
                      options={COMPRESS}
                      value={COMPRESS[form.compress ? 0 : 1] as string}
                      onChange={(choice) => change({ compress: choice === COMPRESS[0] })}
                    />
                  )}
                </Field>
              </>
            )}
            {ENGINEERING_AGENTS.includes(form.name) && (
              <Field label="Engineering skills">
                {(id) => (
                  <Select
                    id={id}
                    options={ENGINEERING}
                    value={ENGINEERING[form.engineeringSkills ? 0 : 1] as string}
                    onChange={(choice) => change({ engineeringSkills: choice === ENGINEERING[0] })}
                  />
                )}
              </Field>
            )}
          </div>
          <ModelNote
            available={available}
            onRefresh={() => void refresh()}
            refreshing={refreshing}
            answered={answered}
          />
        </>
      )}
      {problem && <Notice tone="error">{problem}</Notice>}
      {showSaved &&
        ("written" in saved ? (
          <Notice tone="ok">Saved in moebius.toml.</Notice>
        ) : (
          <Committed commit={saved.commit} />
        ))}
    </Box>
  );
  if (props.broken !== undefined) return <div className="flex flex-col gap-8">{head}</div>;

  const instructions = (
    <Box title="Instructions" className="flex flex-col gap-4">
      {!form.naming && (
        <div className="flex flex-wrap gap-3">
          <Chip selected={!byAgent} onClick={() => setByAgent(false)}>
            Write manually
          </Chip>
          <Chip selected={byAgent} onClick={() => setByAgent(true)}>
            Write with agent
          </Chip>
        </div>
      )}
      {byAgent && !form.naming ? (
        <>
          <TextArea
            mono={false}
            rows={5}
            aria-label="What this agent should do"
            value={wish}
            placeholder="What this agent should do and how. The Moebius agent writes and saves the instructions."
            onChange={(event) => setWish(event.target.value)}
          />
          {ask.isSuccess ? (
            <div className="flex flex-wrap items-center gap-3">
              <Notice tone="ok">The Moebius agent has the request.</Notice>
              <Button asChild variant="secondary" size="sm">
                <Link to={`/chat/${ask.data.id}`}>Open the chat</Link>
              </Button>
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-3">
              {wish.trim() === "" && !ask.isPending && (
                <span className="text-sm text-secondary">
                  Say what the agent should do, then ask
                </span>
              )}
              <Button
                size="sm"
                disabled={wish.trim() === "" || ask.isPending}
                onClick={() => ask.mutate()}
              >
                {ask.isPending ? "Sending" : "Submit"}
              </Button>
            </div>
          )}
        </>
      ) : (
        <TextArea
          rows={14}
          aria-label="Instructions"
          value={form.instructions}
          placeholder="What the agent is told before it starts, in Markdown."
          onChange={(event) => change({ instructions: event.target.value })}
        />
      )}
    </Box>
  );

  return (
    <div className="flex flex-col gap-8">
      {head}
      <div className="grid items-start gap-8 xl:grid-cols-2">
        <div className="flex min-w-0 flex-col gap-8">
          {builtIn && (
            <>
              <Box title="Built-in instructions" aside="Read only" className="flex flex-col gap-4">
                <pre
                  data-testid="built-in-instructions"
                  className="m-0 overflow-x-auto whitespace-pre-wrap rounded-control bg-code p-4 font-mono text-sm text-text"
                >
                  {builtIn.instructions}
                </pre>
              </Box>
              <Box title="Built-in skills" aside="Read only" className="flex flex-col gap-3">
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
          {instructions}
        </div>
        <div className="flex min-w-0 flex-col gap-8">
          {inFile ? (
            <Paths
              heading="Skills"
              aside="Directories, from moebius.toml"
              empty="No skill."
              placeholder="skills/house-style"
              addLabel="Add skill"
              paths={form.skills}
              onChange={(skills) => change({ skills })}
            />
          ) : (
            <>
              <Box
                title="Skills"
                aside={
                  skills === null
                    ? `${form.skills.length} chosen`
                    : `${form.skills.length} of ${skillsOffered.length}`
                }
                className="flex flex-col gap-3"
              >
                {/* A read that failed is not a repository with nothing in it, and the two say opposite things. */}
                {skills === null && (
                  <p className="m-0 text-base text-secondary">
                    The skills of skills/ could not be read.
                  </p>
                )}
                {skills !== null && skillsOffered.length === 0 && form.skills.length === 0 && (
                  <p className="m-0 text-base text-secondary">
                    No skill in skills/ of the repository.
                  </p>
                )}
                {withChosen(
                  skillsOffered.map((skill) => skill.name),
                  form.skills,
                ).map((name) => (
                  <Item
                    key={name}
                    name={
                      <div>
                        <Chip
                          selected={form.skills.includes(name)}
                          onClick={() => change({ skills: toggle(form.skills, name) })}
                        >
                          {name}
                        </Chip>
                      </div>
                    }
                    line={skillsOffered.find((skill) => skill.name === name)?.description}
                  />
                ))}
              </Box>
              <Box
                title="MCP servers"
                aside={
                  mcp === null ? `${form.mcp.length} chosen` : `${form.mcp.length} of ${mcp.length}`
                }
                className="flex flex-col gap-3"
              >
                {mcp === null && (
                  <p className="m-0 text-base text-secondary">
                    The servers of mcp.toml could not be read.
                  </p>
                )}
                {mcp !== null && mcp.length === 0 && form.mcp.length === 0 && (
                  <p className="m-0 text-base text-secondary">
                    No server in mcp.toml of the repository.
                  </p>
                )}
                {withChosen(
                  (mcp ?? []).map((server) => server.name),
                  form.mcp,
                ).map((name) => {
                  const server = mcp?.find((each) => each.name === name);
                  // A name the page cannot check against the file is not a name missing from it.
                  const broken =
                    mcp !== null && (server === undefined || server.error !== undefined);
                  let line: string | undefined;
                  if (server === undefined) line = mcp === null ? undefined : "Not in mcp.toml";
                  else line = server.error ?? server.kind;
                  return (
                    <Item
                      key={name}
                      name={
                        <div>
                          <Chip
                            selected={form.mcp.includes(name)}
                            tone={broken ? "error" : "neutral"}
                            onClick={() => change({ mcp: toggle(form.mcp, name) })}
                          >
                            {name}
                          </Chip>
                        </div>
                      }
                      line={line}
                      red={broken}
                    />
                  );
                })}
              </Box>
            </>
          )}
          <Paths
            heading="Extensions"
            aside="Files in the repository"
            empty="No extension."
            placeholder="extensions/log.ts"
            addLabel="Add extension"
            paths={form.extensions}
            onChange={(extensions) => change({ extensions })}
          />
        </div>
      </div>
    </div>
  );
}
