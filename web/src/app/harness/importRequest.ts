/**
 * What the Moebius agent is asked when a person imports skills or MCP servers (the text of
 * `widgets/import.rs` of the former desktop app, word for word): where the source is, how to fetch it, what to write
 * and what to say at the end, with the person's note. The agent writes what it finds with the same routes the pages
 * use, so every edit is a commit.
 */

export type ImportKind = "skills" | "mcp";

/** Where the agent finds what to import: a zip file stored on the server, or a git repository or a page. */
export type ImportSource = { zip: string } | { link: string };

const SKILLS = [
  "Import skills into the harness configuration repository. Read the skills and MCP servers section of the ",
  "moebius-configure skill first.\n\n{fetch}\n\nA skill is a directory with a SKILL.md. List the skills ",
  'there are with `"$MOEBIUS" api GET /v1/skills` first: never replace an existing skill without saying ',
  "so (ask the person), and never import a skill under a built-in name (`moebius` and the `moebius-` task ",
  "skills); give it another name instead. For each skill, write its description (one line that says when ",
  'to use it) and the text of its SKILL.md after the frontmatter with `"$MOEBIUS" api PUT ',
  "/v1/skills/<name> --body body.json`, building the body with `jq`. Then write each of the skill's other ",
  "text files, such as its scripts, with `jq -Rs '{text: .}' < <file> > body.json` and `\"$MOEBIUS\" api ",
  "PUT /v1/skills/<name>/files/<path> --body body.json`, <path> being the file's path from the skill's ",
  "directory; name a file that is not text, which the person adds through a chat in the harness ",
  'configuration repository. Read each skill back with `"$MOEBIUS" api GET /v1/skills/<name>`, which ',
  "lists its files.\n\nThen list what was imported, ",
  "under which names, and what the person must do: the API keys or programs a skill needs (a key is stored ",
  "on the Settings page, never written into a skill) and which agents should be given the skill.",
].join("");

const MCP = [
  "Import MCP servers into mcp.toml of the harness configuration repository. Read the skills and MCP ",
  "servers section of the moebius-configure skill and `mcp.toml` in reference/harness-configuration.md ",
  "first.\n\n{fetch}\n\nFind how each server is run in its documentation and configuration: the README, ",
  "package.json or pyproject.toml, and examples such as mcp.json or claude_desktop_config.json. List the ",
  'servers there are with `"$MOEBIUS" api GET /v1/mcp-servers` first and never replace one without ',
  'saying so (ask the person). Write each server with `"$MOEBIUS" api PUT /v1/mcp-servers/<name> --body ',
  "body.json`: `command`, `args` and `env` for a server that runs as a command, or `url` and `headers` for ",
  // biome-ignore lint/suspicious/noTemplateCurlyInString: the request names a ${NAME} reference
  "a remote one. Never write a key into it: a key is `${NAME}`, and NAME must be in `harness.mcp_env` of ",
  'the settings (`"$MOEBIUS" settings`); add a name that is missing with `"$MOEBIUS" settings set`, ',
  "keeping the names there, and say whether the server must restart. Read each server back with ",
  '`"$MOEBIUS" api GET /v1/mcp-servers/<name>`.\n\nThen list what was imported, under which names, and ',
  "what the person must do: each key to set in the server's environment under its NAME, the programs the ",
  "server needs on the server, and which agents should be given it.",
].join("");

/** What the Moebius agent is asked to import `source`, with the person's `note`. */
export function importRequest(kind: ImportKind, source: ImportSource, note: string): string {
  const fetchFrom =
    "zip" in source
      ? `The source is the zip file \`${source.zip}\` on the server. Unzip it into a new directory of your working directory (\`unzip -q ${source.zip} -d import\`) and read it there.`
      : `The source is ${source.link}. If it is a git repository, clone it into a new directory of your working directory (\`git clone --depth 1 ${source.link} import\`) and read it there; if it is a page, read it and follow it to the repository or the files it points to.`;
  let asked = (kind === "skills" ? SKILLS : MCP).replace("{fetch}", () => fetchFrom);
  if (note !== "") asked += `\n\nWhat the person said:\n\n${note}`;
  return asked;
}

/**
 * What the Moebius agent is asked to update the built-in skill `name` from where it was copied: the import of skills,
 * told that the person asked for the replacement, which a skill of the harness configuration repository with the same
 * name makes.
 */
export function updateRequest(name: string, source: { url: string; commit: string }): string {
  const note = `Update the skill ${name} from its source. The Moebius copy of it is built in and read only, and a skill of the harness configuration repository with the same name replaces it: so import only the skill ${name}, the directory of the repository whose SKILL.md has \`name: ${name}\`, with all its other files, under the name ${name}. Replacing the built-in copy is what the person asked for. The copy was made at commit ${source.commit}; say which commit you imported and what changed.`;
  return importRequest("skills", { link: source.url }, note);
}

/** What the Moebius agent is asked to write the instructions of the agent `name`, with what the person wants. */
export function instructionsRequest(name: string, wanted: string): string {
  return `Write the instructions of the agent ${name} and save them. Read the agent first with \`"$MOEBIUS" agent show ${name}\`, and the moebius skill's guide on agents. \`"$MOEBIUS" agent save\` replaces the whole agent, so keep its model, protocol, harness, thinking, token budget, tool output settings, skills, MCP servers and extensions as they are, and pass the new instructions with \`--instructions <file>\`. Then say what you wrote.\n\nWhat the person wants ${name} to do:\n\n${wanted}`;
}
