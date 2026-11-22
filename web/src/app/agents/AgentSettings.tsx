/**
 * The Settings tab of an agent's page: the fields of the agent's file as rows, the label at the left and the control at
 * the right (the name while a new agent is named, Model, Protocol, Harness, Runner, Thinking, the token budget per card),
 * the line that says which upstream could not be asked, and what the agent is given: engineering skills, skills and MCP
 * servers, each a row of chips. A server without a harness configuration repository keeps paths in `moebius.toml` in
 * place of the repository's skills and MCP servers. What a built-in agent brings itself is listed read only.
 */
import type { AvailableModels } from "../../api/types/chat";
import { listedModel, offeredLevels } from "../../api/types/chat";
import type { BuiltInAgent, McpServer, Runner, SkillListed } from "../../api/types/harness";
import { Input } from "../../ui/components/Input";
import { Chip } from "../../ui/page/Chip";
import { FieldRow, Rows, Select } from "../../ui/page/Field";
import { ModelField, ModelNote } from "../harness/ModelField";
import { Paths } from "./AgentParts";
import {
  type AgentForm,
  ENGINEERING_SKILLS,
  engineeringOf,
  HARNESSES,
  modelChoices,
  NOT_SET,
  PROTOCOLS,
  THIS_SERVER,
  toggle,
  withChosen,
} from "./model";

/** A row's chips, wrapping, at the right. */
function Chips({ children }: { children: React.ReactNode }) {
  return <div className="flex min-w-0 flex-wrap justify-end gap-2">{children}</div>;
}

export function AgentSettings({
  form,
  change,
  inFile,
  skills,
  mcp,
  runners,
  available,
  builtIn,
  refresh,
}: {
  form: AgentForm;
  change: (patch: Partial<AgentForm>) => void;
  inFile: boolean;
  /** The skills `skills/` of the repository offers, or null when the page could not read them. */
  skills: SkillListed[] | null;
  /** The servers `mcp.toml` of the repository holds, or null when the page could not read them. */
  mcp: McpServer[] | null;
  runners: Runner[];
  available: AvailableModels | undefined;
  builtIn: BuiltInAgent | null;
  refresh: { run: () => void; busy: boolean; answered: boolean };
}) {
  const choices = modelChoices(available);
  const engineering = engineeringOf(form);
  const levels = [NOT_SET, ...offeredLevels(listedModel(available, form.model))];
  if (form.thinking !== null && !levels.includes(form.thinking)) levels.push(form.thinking);
  const places = [THIS_SERVER, ...runners.map((runner) => runner.name)];
  if (form.runner !== null && !places.includes(form.runner)) places.push(form.runner);
  const skillsOffered = (skills ?? []).filter((skill) => !skill.built_in);
  return (
    <div className="flex min-w-0 max-w-4xl flex-col gap-4">
      <Rows>
        {form.naming && (
          <FieldRow label="Name" description="Letters, digits, - and _." wide>
            <Input
              mono
              autoFocus
              value={form.name}
              placeholder="Name of the new agent"
              onChange={(event) => change({ name: event.target.value })}
            />
          </FieldRow>
        )}
        <FieldRow label="Model" wide>
          <ModelField
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
        </FieldRow>
        <FieldRow label="Protocol">
          <Select
            options={[...PROTOCOLS]}
            value={form.protocol}
            onChange={(protocol) => change({ protocol })}
          />
        </FieldRow>
        <FieldRow label="Harness">
          <Select
            options={[...HARNESSES]}
            value={form.harness}
            onChange={(harness) => change({ harness })}
          />
        </FieldRow>
        {(runners.length > 0 || form.runner !== null) && (
          <FieldRow label="Runner">
            <Select
              options={places}
              value={form.runner ?? THIS_SERVER}
              onChange={(runner) => change({ runner: runner === THIS_SERVER ? null : runner })}
            />
          </FieldRow>
        )}
        <FieldRow label="Thinking">
          <Select
            options={levels}
            value={form.thinking ?? NOT_SET}
            onChange={(level) => change({ thinking: level === NOT_SET ? null : level })}
          />
        </FieldRow>
        <FieldRow label="Token budget per card">
          <Input
            value={form.budget}
            placeholder="No limit"
            className="w-40 tabular-nums"
            onChange={(event) => change({ budget: event.target.value })}
          />
        </FieldRow>
        <FieldRow
          label="Engineering skills"
          description={`${engineering.length} of ${ENGINEERING_SKILLS.length}`}
          wide
        >
          <Chips>
            {ENGINEERING_SKILLS.map((skill) => (
              <span key={skill.name} title={skill.line}>
                <Chip
                  selected={engineering.includes(skill.name)}
                  onClick={() => change({ engineeringSkills: toggle(engineering, skill.name) })}
                >
                  {skill.name}
                </Chip>
              </span>
            ))}
          </Chips>
        </FieldRow>
        {builtIn && (
          <FieldRow label="Built-in skills" description="Read only" wide>
            {builtIn.skills.length === 0 ? (
              <span className="text-sm text-muted">No built-in skill.</span>
            ) : (
              <Chips>
                {builtIn.skills.map((skill) => (
                  <span
                    key={skill.name}
                    title={skill.description}
                    className="rounded-control bg-sunken px-2 py-1 font-mono text-sm text-secondary"
                  >
                    {skill.name}
                  </span>
                ))}
              </Chips>
            )}
          </FieldRow>
        )}
        {!inFile && (
          <>
            <FieldRow
              label="Skills"
              description={
                skills === null
                  ? `${form.skills.length} chosen`
                  : `${form.skills.length} of ${skillsOffered.length}`
              }
              wide
            >
              {/* A read that failed is not a repository with nothing in it, and the two say opposite things. */}
              {skills === null ? (
                <span className="text-sm text-secondary">
                  The skills of skills/ could not be read.
                </span>
              ) : skillsOffered.length === 0 && form.skills.length === 0 ? (
                <span className="text-sm text-secondary">
                  No skill in skills/ of the repository.
                </span>
              ) : (
                <Chips>
                  {withChosen(
                    skillsOffered.map((skill) => skill.name),
                    form.skills,
                  ).map((name) => (
                    <span
                      key={name}
                      title={skillsOffered.find((skill) => skill.name === name)?.description}
                    >
                      <Chip
                        selected={form.skills.includes(name)}
                        onClick={() => change({ skills: toggle(form.skills, name) })}
                      >
                        {name}
                      </Chip>
                    </span>
                  ))}
                </Chips>
              )}
            </FieldRow>
            <FieldRow
              label="MCP servers"
              description={
                mcp === null ? `${form.mcp.length} chosen` : `${form.mcp.length} of ${mcp.length}`
              }
              wide
            >
              {mcp === null ? (
                <span className="text-sm text-secondary">
                  The servers of mcp.toml could not be read.
                </span>
              ) : mcp.length === 0 && form.mcp.length === 0 ? (
                <span className="text-sm text-secondary">
                  No server in mcp.toml of the repository.
                </span>
              ) : (
                <Chips>
                  {withChosen(
                    mcp.map((server) => server.name),
                    form.mcp,
                  ).map((name) => {
                    const server = mcp.find((each) => each.name === name);
                    // A name the page cannot check against the file is not a name missing from it.
                    const broken = server === undefined || server.error !== undefined;
                    const line =
                      server === undefined ? "Not in mcp.toml" : (server.error ?? server.kind);
                    return (
                      <span
                        key={name}
                        title={line}
                        className="inline-flex flex-col items-end gap-0.5"
                      >
                        <Chip
                          selected={form.mcp.includes(name)}
                          onClick={() => change({ mcp: toggle(form.mcp, name) })}
                        >
                          {name}
                        </Chip>
                        {broken && <span className="text-xs text-error-text">{line}</span>}
                      </span>
                    );
                  })}
                </Chips>
              )}
            </FieldRow>
          </>
        )}
      </Rows>
      {inFile && (
        <Paths
          heading="Skills"
          aside="Directories, from moebius.toml"
          empty="No skill."
          placeholder="skills/house-style"
          addLabel="Add skill"
          paths={form.skills}
          onChange={(skills) => change({ skills })}
        />
      )}
      <ModelNote
        available={available}
        onRefresh={refresh.run}
        refreshing={refresh.busy}
        answered={refresh.answered}
      />
    </div>
  );
}
