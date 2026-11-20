/**
 * The Import tab of a project's settings, for an owner: a one-off copy of a Jira project into this project
 * (`POST /v1/projects/{id}/import/jira`), which can be run again and makes nothing twice. The form takes the address of
 * the Jira site, the key of its project, the name of the secret the API token is kept under (it starts with `jira-`)
 * and the token itself, which is stored as `PUT /v1/secrets/{name}` does and never sent with the import; with an email
 * the token is Jira Cloud's, without one a personal access token of Jira Server. The route has no dry run, so the page
 * says what the import makes before it is run, and what it made, with every warning, after.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { storeSecret } from "../../api/calls/models";
import { importJira } from "../../api/calls/templates";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import type { JiraImport, JiraReport } from "../../api/types/templates";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { useErrorToast } from "../../ui/lib/useErrorToast";
import { Box } from "../../ui/page/Box";
import { FieldRow, Rows, Select, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";

/** `Jira user = handle` on each line, as the map the route takes; a line that is not one is named. */
export function parseUserMap(text: string): { map: Record<string, string>; bad: string | null } {
  const map: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "") continue;
    const at = trimmed.lastIndexOf("=");
    const jira = at > 0 ? trimmed.slice(0, at).trim() : "";
    const handle = at > 0 ? trimmed.slice(at + 1).trim() : "";
    if (jira === "" || handle === "") return { map, bad: trimmed };
    map[jira] = handle;
  }
  return { map, bad: null };
}

export function ProjectImportTab({ project, canEdit }: { project: Project; canEdit: boolean }) {
  const [url, setUrl] = useState("");
  const [jira, setJira] = useState("");
  const [email, setEmail] = useState("");
  const [secret, setSecret] = useState("jira-token");
  const [token, setToken] = useState("");
  const [users, setUsers] = useState<"people" | "names">("people");
  const [mapText, setMapText] = useState("");
  const queryClient = useQueryClient();
  const { map, bad } = parseUserMap(mapText);
  const run = useMutation<JiraReport>({
    mutationFn: async () => {
      if (token !== "") await storeSecret(secret.trim(), token);
      const body: JiraImport = {
        url: url.trim(),
        project: jira.trim(),
        token_secret: secret.trim(),
        users,
        ...(email.trim() !== "" ? { email: email.trim() } : {}),
        ...(Object.keys(map).length > 0 ? { user_map: map } : {}),
      };
      return importJira(project.id, body);
    },
    onSuccess: () => {
      setToken("");
      // An import makes cards, kinds and people, so what the pages read of them is stale.
      for (const queryKey of [keys.snapshot, keys.issueKinds, keys.projects, keys.people]) {
        void queryClient.invalidateQueries({ queryKey });
      }
    },
  });
  useErrorToast(run.error, "The import failed.");
  if (!canEdit) {
    return (
      <Box title="Import from Jira">
        <p className="m-0 max-w-prose text-base text-secondary">
          Only an owner imports a Jira project, since an import may make people.
        </p>
      </Box>
    );
  }
  const secretOk = /^jira-[A-Za-z0-9_-]+$/.test(secret.trim());
  const ready =
    url.trim() !== "" && jira.trim() !== "" && secretOk && bad === null && !run.isPending;
  return (
    <div className="flex flex-col gap-6">
      <Box>
        <form
          aria-label="Import from Jira"
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (ready) run.mutate();
          }}
        >
          <p className="m-0 max-w-prose text-base text-secondary">
            Copies a Jira project into {project.name}: its issues as cards in the backlog (not
            confirmed, so nothing starts before a person approves), with their comments, labels,
            links, sprints as cycles and epics as milestones. It can be run again and makes nothing
            twice; there is no preview, so a first run on a small project shows what it does.
          </p>
          <Rows>
            <FieldRow
              wide
              label="Jira address"
              description="Such as https://your-site.atlassian.net."
            >
              <Input
                value={url}
                placeholder="https://your-site.atlassian.net"
                autoComplete="off"
                onChange={(event) => setUrl(event.target.value)}
              />
            </FieldRow>
            <FieldRow
              wide
              label="Jira project key"
              description="Letters, digits and _, such as WEB."
            >
              <Input
                value={jira}
                mono
                autoComplete="off"
                onChange={(event) => setJira(event.target.value)}
              />
            </FieldRow>
            <FieldRow
              wide
              label="Email"
              description="For Jira Cloud, whose API token goes with an email. Leave empty for a personal access token of Jira Server or Data Center."
            >
              <Input
                value={email}
                type="email"
                autoComplete="off"
                onChange={(event) => setEmail(event.target.value)}
              />
            </FieldRow>
            <FieldRow
              wide
              label="Secret name"
              description="The name the token is kept under on the server; it starts with jira-."
              error={secret.trim() !== "" && !secretOk ? "The name starts with jira-." : undefined}
            >
              <Input
                value={secret}
                mono
                autoComplete="off"
                onChange={(event) => setSecret(event.target.value)}
              />
            </FieldRow>
            <FieldRow
              wide
              label="API token"
              description="Stored on the server under the secret name and never sent with the import. Leave empty to use the one already stored."
            >
              <Input
                value={token}
                type="password"
                autoComplete="new-password"
                onChange={(event) => setToken(event.target.value)}
              />
            </FieldRow>
            <FieldRow
              wide
              label="People"
              description="“People” makes a disabled person for each Jira user; “Names” makes none and writes the name into what they wrote."
            >
              <Select
                aria-label="People"
                options={[
                  ["people", "Make people"],
                  ["names", "Keep names only"],
                ]}
                value={users}
                onChange={(next) => setUsers(next as "people" | "names")}
              />
            </FieldRow>
            <FieldRow
              wide
              label="Users who exist already"
              description="One per line, Jira account id, email or name, then = and the handle of a person here."
              error={bad ? `“${bad}” is not Jira user = handle.` : undefined}
            >
              <TextArea
                mono
                rows={3}
                value={mapText}
                placeholder="ada@example.com = ada"
                onChange={(event) => setMapText(event.target.value)}
              />
            </FieldRow>
          </Rows>
          <div>
            <Button type="submit" size="sm" disabled={!ready}>
              {run.isPending ? "Importing" : "Import"}
            </Button>
          </div>
        </form>
      </Box>
      {run.isSuccess && <Report report={run.data} />}
    </div>
  );
}

/** What an import made, as the server counted it, and every warning. */
function Report({ report }: { report: JiraReport }) {
  const rows: Array<[string, string]> = [
    ["Issues", String(report.issues)],
    ["Comments", String(report.comments)],
    ["Kinds", String(report.kinds)],
    ["People", String(report.people)],
    ["Cycles", String(report.cycles)],
    [
      "Milestones",
      `${report.milestones.created} made, ${report.milestones.updated} changed, ${report.milestones.unchanged} unchanged`,
    ],
    [
      "Links",
      `${report.links.added} added, ${report.links.removed} removed, ${report.links.unchanged} unchanged, ${report.links.skipped} skipped`,
    ],
  ];
  return (
    <Box title="Imported" testId="jira-report">
      <div className="flex flex-col gap-3">
        <dl className="m-0 grid max-w-xl grid-cols-[8rem_1fr] gap-x-4 gap-y-1 text-base">
          {rows.map(([name, value]) => (
            <div key={name} className="contents">
              <dt className="text-secondary">{name}</dt>
              <dd className="m-0 text-text">{value}</dd>
            </div>
          ))}
        </dl>
        {report.warnings.length === 0 ? (
          <p className="m-0 text-sm text-secondary">No warnings.</p>
        ) : (
          <Notice>
            <span className="font-semibold">
              {report.warnings.length === 1 ? "1 warning" : `${report.warnings.length} warnings`}
            </span>
            {/* biome-ignore lint/a11y/noRedundantRoles: preflight takes the bullets off, and WebKit then drops the list */}
            <ul role="list" aria-label="Warnings" className="m-0 mt-1 flex flex-col gap-1 p-0">
              {report.warnings.map((warning, index) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: warnings can repeat, and the order is fixed
                <li key={index}>{warning}</li>
              ))}
            </ul>
          </Notice>
        )}
      </div>
    </Box>
  );
}
