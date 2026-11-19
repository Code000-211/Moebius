/**
 * The right column of the agent editor: the skills, MCP servers and extensions the agent is given. A server without a
 * harness configuration repository keeps paths in `moebius.toml` in place of the repository's skills and MCP servers.
 * Browse Pi packages opens the package sheet on the pi.dev catalog, for this agent.
 */

import type { McpServer, SkillListed } from "../../api/types/harness";
import { usePackageSheet } from "../../packages/PackageSheetContext";
import { Box } from "../../ui/page/Box";
import { Chip } from "../../ui/page/Chip";
import { Item, Paths } from "./AgentParts";
import { type AgentForm, toggle, withChosen } from "./model";

export function AgentFiles({
  form,
  change,
  inFile,
  skills,
  mcp,
}: {
  form: AgentForm;
  change: (patch: Partial<AgentForm>) => void;
  inFile: boolean;
  /** The skills `skills/` of the repository offers, or null when the page could not read them. */
  skills: SkillListed[] | null;
  /** The servers `mcp.toml` of the repository holds, or null when the page could not read them. */
  mcp: McpServer[] | null;
}) {
  const packages = usePackageSheet();
  const skillsOffered = (skills ?? []).filter((skill) => !skill.built_in);
  return (
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
              <p className="m-0 text-base text-secondary">No skill in skills/ of the repository.</p>
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
              const broken = mcp !== null && (server === undefined || server.error !== undefined);
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
        browse={{
          label: "Browse Pi packages",
          // A package is named for an agent that exists; a new one has no files to name it in yet.
          onClick: () => packages.open("pi", form.exists ? form.name : null),
        }}
      />
    </div>
  );
}
