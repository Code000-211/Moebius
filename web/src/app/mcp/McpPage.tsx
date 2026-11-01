/**
 * The MCP servers page: the servers of `mcp.toml` that agents can name, and the editor of the one chosen. An edit is
 * `PUT /v1/mcp-servers/{name}`, which the server makes a commit in the harness configuration repository. A value may
 * be `${NAME}`, which the server takes from its environment when a run starts; the page never shows what it stands
 * for. A server without a harness configuration repository lists no servers, and the page then offers to set one up.
 * The server chosen is in the address (`?server=`, or `?new=1`).
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
import { Loader } from "../../ui/components/Loader";
import {
  Box,
  Committed,
  EditorHead,
  Field,
  ListCard,
  ListEditor,
  Notice,
  Select,
  TextArea,
  TextInput,
} from "../harness/form";
import { HarnessSetup, WHY_MUTED } from "../harness/HarnessSetup";
import { ImportPanel } from "../harness/ImportPanel";
import { Header } from "../shell/Page";
import { blankForm, formOf, type McpForm, mcpRequest } from "./model";

// biome-ignore lint/suspicious/noTemplateCurlyInString: a value may be a ${NAME} reference, which the server resolves
const REFERENCE = "${NAME}";
const VALUE_HINT = `value or ${REFERENCE}`;
const COMMAND = "Command";
const URL = "URL";

export function McpPage() {
  const [params, setParams] = useSearchParams();
  const [importing, setImporting] = useState(false);
  const [saved, setSaved] = useState<{ name: string; commit: string | null } | null>(null);
  const servers = useQuery({ queryKey: keys.mcpServers, queryFn: mcpServers });
  // The skills say whether the server has a harness configuration repository: they list the built-in ones with it.
  const skills = useQuery({ queryKey: keys.skills, queryFn: skillList });
  const chosen = params.get("server");
  const creating = params.get("new") === "1";
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
  const editable = skills.isSuccess && skills.data.length > 0;
  const noRepository = skills.isSuccess && skills.data.length === 0;

  return (
    <>
      <div className="flex items-start justify-between gap-base">
        <Header breadcrumb="Build" title="MCP servers" />
        <div className="flex gap-plusfifty" title={editable ? undefined : WHY_MUTED}>
          <Button
            variant="outline"
            size="sm"
            disabled={!editable}
            onClick={() => setImporting(true)}
          >
            Import
          </Button>
          <Button size="sm" disabled={!editable} onClick={() => select(null, true)}>
            <PlusIcon className="size-icon-sm" weight="bold" />
            New server
          </Button>
        </div>
      </div>
      <p className="m-0 mb-plusfifty text-sm text-normal">
        The servers of mcp.toml in the harness repository; agents name them to get their tools. An
        edit is a commit.
      </p>
      {skills.isPending && <Loader message="Reading the MCP servers…" className="py-double" />}
      {noRepository && <HarnessSetup />}
      {editable && (
        <div className="flex flex-col gap-double">
          {importing && <ImportPanel kind="mcp" onClose={() => setImporting(false)} />}
          <ListEditor
            list={
              servers.data && servers.data.length === 0 ? (
                <Box title="MCP servers">
                  <p className="m-0 text-sm text-normal">No server yet.</p>
                </Box>
              ) : (
                (servers.data ?? []).map((server) => (
                  <ListCard
                    key={server.name}
                    name={server.name}
                    line={server.error ?? server.kind}
                    red={server.error !== undefined}
                    chosen={!creating && chosen === server.name}
                    onClick={() => {
                      setSaved(null);
                      select(server.name);
                    }}
                  />
                ))
              )
            }
          >
            {creating ? (
              <ServerEditor
                key="new"
                name=""
                naming
                initial={blankForm()}
                onNamed={(name, commit) => {
                  setSaved({ name, commit });
                  select(name);
                }}
                onDeleted={() => select(null)}
              />
            ) : chosen !== null ? (
              <ChosenServer
                key={chosen}
                name={chosen}
                saved={saved?.name === chosen ? saved.commit : undefined}
                onNamed={(name, commit) => {
                  setSaved({ name, commit });
                  select(name);
                }}
                onDeleted={() => select(null)}
              />
            ) : (
              <Box>
                <p className="m-0 text-sm text-low">Pick a server to edit it.</p>
              </Box>
            )}
          </ListEditor>
        </div>
      )}
    </>
  );
}

function ChosenServer({
  name,
  saved,
  onNamed,
  onDeleted,
}: {
  name: string;
  saved: string | null | undefined;
  onNamed: (name: string, commit: string | null) => void;
  onDeleted: () => void;
}) {
  const detail = useQuery({
    queryKey: keys.mcpServer(name),
    queryFn: () => mcpServer(name),
    retry: false,
  });
  if (detail.isPending) return <p className="m-0 text-sm text-normal">Reading the server…</p>;
  return (
    <ServerEditor
      name={name}
      naming={false}
      initial={detail.data ? formOf(name, detail.data) : blankForm(name)}
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
  readError,
  saved,
  onNamed,
  onDeleted,
}: {
  name: string;
  naming: boolean;
  initial: McpForm;
  readError?: string;
  saved?: string | null;
  onNamed: (name: string, commit: string | null) => void;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<McpForm>({ ...initial, name: initialName });
  const [naming, setNaming] = useState(initialNaming);
  const [exists, setExists] = useState(!initialNaming);
  const [notice, setNotice] = useState<{ error: string } | { commit: string | null } | null>(
    readError ? { error: readError } : saved !== undefined ? { commit: saved } : null,
  );
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
      setForm((held) => ({ ...held, name: saved.name }));
      setNaming(false);
      setExists(true);
      setNotice({ commit: saved.commit ?? null });
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
      await queryClient.invalidateQueries({ queryKey: keys.mcpServers });
      onDeleted();
    },
    onError: (error) => setNotice({ error: error.message }),
  });

  return (
    <div className="flex flex-col gap-double">
      <Box>
        <EditorHead
          name={form.name}
          naming={naming}
          exists={exists}
          deleteLabel="Delete server"
          namePlaceholder="Name of the new server"
          onName={(name) => change({ name })}
          saving={save.isPending}
          canSave
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
      <Box>
        <Field label="Kind" className="max-w-xs">
          {(id) => (
            <Select
              id={id}
              options={[COMMAND, URL]}
              value={form.remote ? URL : COMMAND}
              onChange={(kind) => change({ remote: kind === URL })}
            />
          )}
        </Field>
        {form.remote ? (
          <Field label="URL">
            {(id) => (
              <TextInput
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
            <Field label="Command">
              {(id) => (
                <TextInput
                  id={id}
                  mono
                  value={form.command}
                  placeholder="docs-server"
                  onChange={(event) => change({ command: event.target.value })}
                />
              )}
            </Field>
            <Field label="Arguments, one per line">
              {(id) => (
                <TextArea
                  id={id}
                  rows={4}
                  value={form.args}
                  placeholder={"--root\n/srv/docs"}
                  onChange={(event) => change({ args: event.target.value })}
                />
              )}
            </Field>
          </>
        )}
        <div className="flex items-baseline justify-between">
          <h3 className="m-0 text-[13px] font-semibold text-high">
            {form.remote ? "Headers" : "Environment"}
          </h3>
          <span className="font-mono text-[11.5px] text-low">Values may be {REFERENCE}</span>
        </div>
        {form.rows.map((row, at) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the rows have no identity but their place
          <div key={at} className="flex items-center gap-base">
            <TextInput
              mono
              aria-label={`Name ${at + 1}`}
              value={row[0]}
              placeholder="NAME"
              onChange={(event) => setRow(at, 0, event.target.value)}
            />
            <TextInput
              mono
              aria-label={`Value ${at + 1}`}
              value={row[1]}
              placeholder={VALUE_HINT}
              onChange={(event) => setRow(at, 1, event.target.value)}
            />
            <Button
              variant="outline"
              size="xs"
              aria-label={`Remove row ${at + 1}`}
              onClick={() => change({ rows: form.rows.filter((_, place) => place !== at) })}
            >
              Remove
            </Button>
          </div>
        ))}
        <div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => change({ rows: [...form.rows, ["", ""]] })}
          >
            {form.remote ? "Add header" : "Add variable"}
          </Button>
        </div>
      </Box>
    </div>
  );
}
