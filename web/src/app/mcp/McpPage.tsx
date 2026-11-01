/**
 * The MCP servers page: the servers of `mcp.toml` that agents can name, and the editor of the one chosen. An edit is
 * `PUT /v1/mcp-servers/{name}`, which the server makes a commit in the harness configuration repository. A value may
 * be `${NAME}`, which the server takes from its environment when a run starts; the page never shows what it stands
 * for. A server without a harness configuration repository lists no servers, and the page then offers to set one up.
 * The server chosen is in the address (`?server=`, or `?new=1`), and what was typed for one is kept while the page
 * lives, so moving along the list does not throw it away.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useSearchParams } from "react-router";
import {
  deleteMcpServer,
  mcpServer,
  mcpServers,
  saveMcpServer,
  skillList,
} from "../../api/calls/harness";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Loader } from "../../ui/components/Loader";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { Field, Select, TextArea } from "../../ui/page/Field";
import { chosenOrFirst, ListCard, ListDetail } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { Committed, EditorHead, useDraft, useDrafts } from "../harness/form";
import { HarnessSetup, WHY_MUTED } from "../harness/HarnessSetup";
import { ImportPanel } from "../harness/ImportPanel";
import { blankForm, formOf, type McpForm, mcpRequest } from "./model";

// biome-ignore lint/suspicious/noTemplateCurlyInString: a value may be a ${NAME} reference, which the server resolves
const REFERENCE = "${NAME}";
const VALUE_HINT = `value or ${REFERENCE}`;
const COMMAND = "Command";
const URL = "URL";
/** The draft of a new server, which has no name to hold it yet. */
const NEW = "\u0000new";

export function McpPage() {
  const [params, setParams] = useSearchParams();
  const [importing, setImporting] = useState(false);
  const [saved, setSaved] = useState<{ name: string; commit: string | null } | null>(null);
  // What was typed for each server, held while the page lives, so choosing another row keeps the draft.
  const drafts = useDrafts<McpForm | null>();
  const newDraft = { hold: drafts.hold(NEW), drop: () => drafts.drop(NEW) };
  const servers = useQuery({ queryKey: keys.mcpServers, queryFn: mcpServers });
  // The skills say whether the server has a harness configuration repository: they list the built-in ones with it.
  const skills = useQuery({ queryKey: keys.skills, queryFn: skillList });
  const chosen = params.get("server");
  const creating = params.get("new") === "1";
  // The first server is opened rather than an empty pane beside a list that has one in it.
  const picked = chosenOrFirst(chosen, creating, servers.data?.map((server) => server.name) ?? []);
  const select = (server: string | null, fresh = false) =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        if (server === null) next.delete("server");
        else next.set("server", server);
        if (fresh) next.set("new", "1");
        else next.delete("new");
        return next;
      },
      { replace: true },
    );
  const startNew = () => {
    setSaved(null);
    select(null, true);
  };
  const editable = skills.isSuccess && skills.data.length > 0;
  const noRepository = skills.isSuccess && skills.data.length === 0;
  // Why the two buttons that change servers are quiet. It is printed beside them, so that a keyboard and a screen
  // reader reach it too: a `disabled` button is out of the tab order and carries no tooltip there.
  const whyQuiet = editable
    ? undefined
    : skills.isError
      ? "The harness configuration could not be read, so none can be added yet."
      : skills.isPending
        ? undefined
        : WHY_MUTED;
  const none = servers.isSuccess && servers.data.length === 0;
  const newForm = blankForm();

  return (
    <>
      <PageHeader
        title="MCP servers"
        context="The tools an agent can be given beyond its own."
        actions={
          <div className="flex flex-wrap items-center gap-3">
            {whyQuiet && <span className="text-sm text-secondary">{whyQuiet}</span>}
            <Button
              variant="secondary"
              size="sm"
              disabled={!editable}
              onClick={() => setImporting(true)}
            >
              Import
            </Button>
            <Button size="sm" disabled={!editable} onClick={startNew}>
              <PlusIcon className="size-4" weight="bold" />
              New server
            </Button>
          </div>
        }
      />
      {skills.isPending && <Loader message="Reading the MCP servers" className="py-8" />}
      {skills.isError && (
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
          {`Could not read the harness configuration, so no server can be listed. ${skills.error.message}`}
        </Notice>
      )}
      {noRepository && <HarnessSetup />}
      {editable && (
        <div className="flex flex-col gap-8">
          {importing && <ImportPanel kind="mcp" onClose={() => setImporting(false)} />}
          <ListDetail
            list={
              servers.isPending ? (
                <Loader message="Reading the servers" className="py-8" />
              ) : servers.isError ? (
                <Notice
                  tone="error"
                  action={
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={servers.isFetching}
                      onClick={() => void servers.refetch()}
                    >
                      Try again
                    </Button>
                  }
                >
                  {`Could not read the servers. ${servers.error.message}`}
                </Notice>
              ) : (
                servers.data.map((server) => (
                  <ListCard
                    key={server.name}
                    title={server.name}
                    line={server.error ?? server.kind}
                    alert={server.error !== undefined}
                    selected={!creating && picked === server.name}
                    onClick={() => {
                      setSaved(null);
                      select(server.name);
                    }}
                  />
                ))
              )
            }
            detail={
              creating ? (
                <ServerEditor
                  key="new"
                  name=""
                  naming
                  initial={drafts.take(NEW, () => null) ?? newForm}
                  stored={newForm}
                  drafts={newDraft}
                  onNamed={(name, commit) => {
                    drafts.drop(NEW);
                    setSaved({ name, commit });
                    select(name);
                  }}
                  onDeleted={() => {
                    drafts.drop(NEW);
                    select(null);
                  }}
                />
              ) : picked !== null ? (
                <ChosenServer
                  key={picked}
                  name={picked}
                  draft={drafts.take(picked, () => null)}
                  drafts={{ hold: drafts.hold(picked), drop: () => drafts.drop(picked) }}
                  saved={saved?.name === picked ? saved.commit : undefined}
                  onNamed={(name, commit) => {
                    setSaved({ name, commit });
                    select(name);
                  }}
                  onDeleted={() => select(null)}
                />
              ) : none ? (
                <Empty
                  title="No servers yet"
                  body="A server gives an agent tools it cannot reach on its own. Add one to name its command or its address."
                  action={
                    <Button size="sm" onClick={startNew}>
                      <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
                      New server
                    </Button>
                  }
                />
              ) : (
                <Empty
                  title="Nothing chosen"
                  body="Pick a server on the left to change how it runs and the values it is given."
                />
              )
            }
          />
        </div>
      )}
    </>
  );
}

/** Reads the definition of the server `name`, and opens the editor on it, or on what was typed for it. */
function ChosenServer({
  name,
  draft,
  drafts,
  saved,
  onNamed,
  onDeleted,
}: {
  name: string;
  draft: McpForm | null;
  drafts: { hold: (form: McpForm) => void; drop: () => void };
  saved: string | null | undefined;
  onNamed: (name: string, commit: string | null) => void;
  onDeleted: () => void;
}) {
  const detail = useQuery({
    queryKey: keys.mcpServer(name),
    queryFn: () => mcpServer(name),
    retry: false,
  });
  if (detail.isPending) return <p className="m-0 text-base text-secondary">Reading the server</p>;
  // What the file holds, which a draft is not.
  const stored = detail.data ? formOf(name, detail.data) : blankForm(name);
  return (
    <ServerEditor
      name={name}
      naming={false}
      initial={draft ?? stored}
      stored={stored}
      drafts={drafts}
      // A server that cannot be read is in the file, so it can be replaced or deleted.
      readError={detail.isError ? detail.error.message : undefined}
      saved={saved}
      onNamed={onNamed}
      onDeleted={onDeleted}
    />
  );
}

function ServerEditor({
  name: initialName,
  naming: initialNaming,
  initial,
  stored,
  readError,
  saved,
  drafts,
  onNamed,
  onDeleted,
}: {
  name: string;
  naming: boolean;
  initial: McpForm;
  /** What the file holds for this server, so Save can say whether anything waits for it even when the editor
   * reopened on a draft. */
  stored: McpForm;
  readError?: string;
  saved?: string | null;
  /** Where the page keeps what is typed here, so choosing another row does not lose it. */
  drafts?: { hold: (form: McpForm) => void; drop: () => void };
  onNamed: (name: string, commit: string | null) => void;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<McpForm>({ ...initial, name: initialName });
  const [baseline, setBaseline] = useState<McpForm>(stored);
  const [naming, setNaming] = useState(initialNaming);
  const [exists, setExists] = useState(!initialNaming);
  const [notice, setNotice] = useState<{ error: string } | { commit: string | null } | null>(
    readError ? { error: readError } : saved !== undefined ? { commit: saved } : null,
  );
  const dirty = JSON.stringify(form) !== JSON.stringify(baseline);
  useDraft(form, dirty, drafts);
  const change = (patch: Partial<McpForm>) => {
    setForm((held) => ({ ...held, ...patch }));
    setNotice(null);
  };
  const setRow = (at: number, column: 0 | 1, value: string) =>
    change({
      rows: form.rows.map((row, place) =>
        place === at
          ? ((column === 0 ? [value, row[1]] : [row[0], value]) as [string, string])
          : row,
      ),
    });
  const save = useMutation({
    mutationFn: () => {
      const request = mcpRequest(form);
      if (typeof request === "string") throw new Error(request);
      return saveMcpServer(request.name, request.definition);
    },
    onSuccess: async (saved) => {
      const next: McpForm = { ...form, name: saved.name };
      setForm(next);
      setBaseline(next);
      setNaming(false);
      setExists(true);
      setNotice({ commit: saved.commit ?? null });
      drafts?.drop();
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.mcpServers }),
        queryClient.invalidateQueries({ queryKey: keys.mcpServer(saved.name) }),
      ]);
      onNamed(saved.name, saved.commit ?? null);
    },
    onError: (error) => setNotice({ error: error.message }),
  });
  const remove = useMutation({
    mutationFn: () => deleteMcpServer(form.name),
    onSuccess: async () => {
      drafts?.drop();
      await queryClient.invalidateQueries({ queryKey: keys.mcpServers });
      onDeleted();
    },
    onError: (error) => setNotice({ error: error.message }),
  });

  const remote = form.remote;
  const values = remote ? "headers" : "variables";

  return (
    <div className="flex flex-col gap-8">
      <Box className="flex flex-col gap-4">
        <EditorHead
          name={form.name}
          naming={naming}
          exists={exists}
          deleteLabel="Delete server"
          namePlaceholder="Name of the new server"
          onName={(name) => change({ name })}
          saving={save.isPending}
          canSave
          dirty={dirty}
          onSave={() => save.mutate()}
          onDelete={() => remove.mutate()}
          deleting={remove.isPending}
        />
        {notice &&
          ("error" in notice ? (
            <Notice tone="error">{notice.error}</Notice>
          ) : (
            <Committed commit={notice.commit} />
          ))}
      </Box>

      <Box
        title="How it runs"
        aside={remote ? "Reached over a URL" : "Started as a command"}
        className="flex flex-col gap-4"
      >
        <Field label="Kind" className="max-w-xs">
          {(id) => (
            <Select
              id={id}
              options={[COMMAND, URL]}
              value={remote ? URL : COMMAND}
              onChange={(kind) => change({ remote: kind === URL })}
            />
          )}
        </Field>
        {remote ? (
          <Field
            label="URL"
            hint="The address the server answers at, such as https://search.example.com/mcp."
          >
            {(id) => (
              <Input
                id={id}
                mono
                value={form.url}
                placeholder="https://search.example.com/mcp"
                onChange={(event) => change({ url: event.target.value })}
              />
            )}
          </Field>
        ) : (
          <>
            <Field label="Command" hint="The program the server starts.">
              {(id) => (
                <Input
                  id={id}
                  mono
                  value={form.command}
                  placeholder="docs-server"
                  onChange={(event) => change({ command: event.target.value })}
                />
              )}
            </Field>
            <Field label="Arguments, one per line" hint="Blank lines are left out.">
              {(id) => (
                <TextArea
                  id={id}
                  mono
                  rows={4}
                  value={form.args}
                  placeholder={"--root\n/srv/docs"}
                  onChange={(event) => change({ args: event.target.value })}
                />
              )}
            </Field>
          </>
        )}
      </Box>

      <Box
        title={remote ? "Headers" : "Environment"}
        aside={
          <>
            Values may be <span className="font-mono text-sm">{REFERENCE}</span>
          </>
        }
        className="flex flex-col gap-4"
      >
        {form.rows.length === 0 && (
          <p className="m-0 text-base text-secondary">
            {`No ${values} yet. A ${remote ? "header" : "variable"} the server needs from the machine it runs on goes here.`}
          </p>
        )}
        {form.rows.map((row, at) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the rows have no identity but their place
          <div key={at} className="flex flex-wrap items-center gap-3">
            <Input
              mono
              className="min-w-40 flex-1"
              aria-label={`Name ${at + 1}`}
              value={row[0]}
              placeholder="NAME"
              onChange={(event) => setRow(at, 0, event.target.value)}
            />
            <Input
              mono
              className="min-w-40 flex-1"
              aria-label={`Value ${at + 1}`}
              value={row[1]}
              placeholder={VALUE_HINT}
              onChange={(event) => setRow(at, 1, event.target.value)}
            />
            <Button
              variant="danger"
              size="sm"
              aria-label={`Remove row ${at + 1}`}
              onClick={() => change({ rows: form.rows.filter((_, place) => place !== at) })}
            >
              Remove
            </Button>
          </div>
        ))}
        <div>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => change({ rows: [...form.rows, ["", ""]] })}
          >
            {remote ? "Add header" : "Add variable"}
          </Button>
        </div>
      </Box>
    </div>
  );
}
