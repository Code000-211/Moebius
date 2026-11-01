/**
 * The Models page: where model requests go (the router's upstreams with their API keys, and the logins of those that
 * use OAuth), the model classes, the agents' models (read only; agents are edited on the Agents page), and the
 * teacher and the decision model, which may be left out. It edits `moebius.toml` through `PUT /v1/settings`: the form
 * is made of the keys `GET /v1/settings` tells and Save sends only what was changed (`model.ts`). An API key typed
 * here goes to `PUT /v1/secrets/{name}` first, once, and the file names it; the key is never read back and the
 * field is empty again after the save. A key the running server uses only after a restart makes a banner with a
 * Restart button.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  availableModels,
  models as readModels,
  providers as readProviders,
  restartServer,
  saveSettings,
  serverSettings,
  storeSecret,
} from "../../api/calls/models";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Loader } from "../../ui/components/Loader";
import { decisionChoices, modelChoices } from "../agents/model";
import { Box, Field, Notice, Select, TextInput } from "../harness/form";
import { ModelField, ModelNote } from "../harness/ModelField";
import { Header } from "../shell/Page";
import { LoginRow, type LoginStep } from "./LoginRow";
import {
  blankUpstream,
  type ClassForm,
  type Field as FieldForm,
  KEY_MODES,
  type KeyChoice,
  type KeyMode,
  type ModelsForm,
  modelsChanges,
  modelsDirty,
  nextUid,
  type OptionalForm,
  PROTOCOLS,
  readForm,
  type UpstreamForm,
} from "./model";
import { ProviderPanel } from "./ProviderPanel";

type Notice_ = { kind: "saved" } | { kind: "same" } | { kind: "failed"; text: string };

export function ModelsPage() {
  const queryClient = useQueryClient();
  const models = useQuery({ queryKey: keys.modelsInUse, queryFn: readModels });
  const server = useQuery({ queryKey: keys.settings, queryFn: serverSettings });
  const available = useQuery({
    queryKey: keys.availableDecision,
    queryFn: () => availableModels(false, true),
  });
  const providers = useQuery({ queryKey: keys.providers, queryFn: readProviders });
  const [form, setForm] = useState<ModelsForm | null>(null);
  const [notice, setNotice] = useState<Notice_ | null>(null);
  const [saving, setSaving] = useState(false);
  const [logins, setLogins] = useState<Record<number, LoginStep>>({});
  const [choosing, setChoosing] = useState(false);
  const [adding, setAdding] = useState(false);
  const [addProblem, setAddProblem] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [restartedAt, setRestartedAt] = useState<number | null>(null);

  // What is being edited is kept when the server tells the same again; the form is made anew when nothing was changed.
  const updated = server.dataUpdatedAt;
  const read = server.data;
  useEffect(() => {
    if (!read) return;
    setForm((held) => (held === null || !modelsDirty(held) ? readForm(read) : held));
  }, [read]);

  if (models.isError) return <Failed message={models.error.message} />;
  if (server.isError) return <Failed message={server.error.message} />;
  if (models.isPending || server.isPending || form === null) {
    return (
      <>
        <Header breadcrumb="Setup" title="Models" />
        <Loader message="Reading the models…" className="py-double" />
      </>
    );
  }
  const data = models.data;
  const settings = server.data;
  const secrets = settings.secrets;
  const edit = (change: (form: ModelsForm) => void) => {
    setForm((held) => {
      if (!held) return held;
      const next = structuredClone(held);
      change(next);
      return next;
    });
    setNotice(null);
  };

  const patchUpstream = (place: number, patch: Partial<UpstreamForm>) =>
    edit((next) => void Object.assign(next.upstreams[place] ?? {}, patch));
  const patchKey = (place: number, patch: Partial<KeyChoice>) =>
    edit((next) => void Object.assign(next.upstreams[place]?.key ?? {}, patch));
  const patchClass = (place: number, patch: Partial<ClassForm>) =>
    edit((next) => void Object.assign(next.classes[place] ?? {}, patch));
  const patchOptional = (which: "teacher" | "decisionModel", key: string, text: string) =>
    edit(
      (next) =>
        void Object.assign(next[which].fields.find((field) => field.key === key) ?? {}, { text }),
    );

  async function save() {
    if (!form) return;
    const plan = modelsChanges(form, secrets);
    if (typeof plan === "string") {
      setNotice({ kind: "failed", text: plan });
      return;
    }
    if (plan.store.length === 0 && Object.keys(plan.set).length === 0 && plan.remove.length === 0) {
      setNotice({ kind: "same" });
      return;
    }
    setSaving(true);
    setNotice(null);
    try {
      // The keys go first, since the file names them and the server refuses a name it does not have.
      for (const [name, key] of plan.store) await storeSecret(name, key);
      if (Object.keys(plan.set).length > 0 || plan.remove.length > 0) {
        await saveSettings(plan.set, plan.remove);
      }
      const fresh = await queryClient.fetchQuery({
        queryKey: keys.settings,
        queryFn: serverSettings,
        staleTime: 0,
      });
      setForm(readForm(fresh));
      setNotice({ kind: "saved" });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.modelsInUse }),
        queryClient.invalidateQueries({ queryKey: keys.availableDecision }),
        queryClient.invalidateQueries({ queryKey: keys.models }),
      ]);
    } catch (error) {
      setNotice({ kind: "failed", text: error instanceof Error ? error.message : "cannot save" });
    }
    setSaving(false);
  }

  async function addUpstream(upstream: Record<string, unknown>) {
    const place = form?.originalUpstreams.length ?? 0;
    setAdding(true);
    setAddProblem(null);
    try {
      await saveSettings({ [`router.upstreams.${place}`]: upstream }, []);
      const fresh = await queryClient.fetchQuery({
        queryKey: keys.settings,
        queryFn: serverSettings,
        staleTime: 0,
      });
      setForm(readForm(fresh));
      await queryClient.invalidateQueries({ queryKey: keys.modelsInUse });
      // The row of the new upstream asks for the confirmation of the warning.
      setLogins((held) => ({ ...held, [place]: { kind: "confirming" } }));
      setChoosing(false);
    } catch (error) {
      setAddProblem(error instanceof Error ? error.message : "cannot add the upstream");
    }
    setAdding(false);
  }

  async function refresh() {
    setRefreshing(true);
    try {
      queryClient.setQueryData(keys.availableDecision, await availableModels(true, true));
    } finally {
      setRefreshing(false);
    }
  }

  async function restart() {
    setRestartedAt(updated);
    try {
      await restartServer();
    } catch (error) {
      setRestartedAt(null);
      setNotice({
        kind: "failed",
        text: error instanceof Error ? error.message : "cannot restart",
      });
    }
  }
  const restarting = restartedAt !== null && restartedAt === updated;

  const teacherChoices = modelChoices(available.data);
  const decisionModelChoices = decisionChoices(available.data, data.decision_model?.url ?? "");
  const note = (first: boolean) =>
    first ? (
      <ModelNote
        available={available.data}
        onRefresh={() => void refresh()}
        refreshing={refreshing}
      />
    ) : null;

  return (
    <>
      <div className="flex items-start justify-between gap-base">
        <Header breadcrumb="Setup" title="Models" />
        <Button size="sm" disabled={saving} onClick={() => void save()}>
          {saving ? "Saving…" : "Save"}
        </Button>
      </div>
      <p className="m-0 mb-plusfifty text-sm text-normal">
        Where model requests go, and the models that check and decide. Keys are never shown.
      </p>
      <div className="flex flex-col gap-double">
        {restarting ? (
          <Banner>Restarting the server. The app reconnects by itself.</Banner>
        ) : settings.restart_needed ? (
          <Banner
            action={
              <Button size="sm" onClick={() => void restart()}>
                Restart
              </Button>
            }
          >
            Restart the server to use: {settings.restart_keys.join(", ")}
          </Banner>
        ) : null}
        {notice?.kind === "saved" && <Notice tone="ok">Saved.</Notice>}
        {notice?.kind === "same" && <Notice tone="ok">No changes to save.</Notice>}
        {notice?.kind === "failed" && <Notice tone="error">{notice.text}</Notice>}

        <Box title="Upstreams">
          {form.upstreams.length === 0 ? (
            <p className="m-0 text-sm text-normal">
              No upstream is configured; every model request fails.
            </p>
          ) : (
            form.upstreams.map((upstream, place) => {
              const oauth =
                upstream.origin === null ? undefined : data.upstreams[upstream.origin]?.oauth;
              return (
                <div
                  key={upstream.uid}
                  className="flex flex-col gap-plusfifty"
                  data-testid="upstream"
                >
                  <div className="grid items-start gap-plusfifty md:grid-cols-[8rem_minmax(0,1fr)_minmax(0,1.2fr)_auto]">
                    <Field label="Protocol">
                      {(id) => (
                        <Select
                          id={id}
                          options={[...PROTOCOLS]}
                          value={upstream.protocol}
                          onChange={(protocol) => patchUpstream(place, { protocol })}
                        />
                      )}
                    </Field>
                    <Field label="Base URL">
                      {(id) => (
                        <TextInput
                          id={id}
                          mono
                          value={upstream.baseUrl}
                          placeholder="http://127.0.0.1:8080"
                          onChange={(event) =>
                            patchUpstream(place, { baseUrl: event.target.value })
                          }
                        />
                      )}
                    </Field>
                    {upstream.oauth !== null ? (
                      <Field label="Key">
                        {() => (
                          <span className="py-2 text-sm text-normal">
                            None: it logs in with OAuth
                          </span>
                        )}
                      </Field>
                    ) : (
                      <KeyField
                        label={`Key of upstream ${place + 1}`}
                        choice={upstream.key}
                        secrets={secrets}
                        onChange={(change) => patchKey(place, change)}
                      />
                    )}
                    <div className="md:pt-[22px]">
                      <Button
                        variant="outline"
                        size="sm"
                        aria-label={`Remove upstream ${place + 1}`}
                        onClick={() => edit((next) => void next.upstreams.splice(place, 1))}
                      >
                        Remove
                      </Button>
                    </div>
                  </div>
                  {oauth && upstream.origin !== null && (
                    <LoginRow
                      place={upstream.origin}
                      oauth={oauth}
                      step={logins[upstream.origin]}
                      onStep={(step) =>
                        setLogins((held) => {
                          const next = { ...held };
                          if (step === undefined) delete next[upstream.origin as number];
                          else next[upstream.origin as number] = step;
                          return next;
                        })
                      }
                    />
                  )}
                </div>
              );
            })
          )}
          <div className="flex gap-plusfifty">
            <Button
              variant="outline"
              size="sm"
              onClick={() => edit((next) => void next.upstreams.push(blankUpstream()))}
            >
              Add upstream
            </Button>
            <Button variant="outline" size="sm" onClick={() => setChoosing(true)}>
              Log in to a provider
            </Button>
          </div>
          {choosing && (
            <div className="border-t border-line pt-plusfifty">
              <ProviderPanel
                providers={providers.data ?? []}
                adding={adding}
                problem={addProblem}
                onAdd={(upstream) => void addUpstream(upstream)}
                onCancel={() => {
                  setChoosing(false);
                  setAddProblem(null);
                }}
              />
            </div>
          )}
        </Box>

        <Box title="Model classes">
          <p className="m-0 text-sm text-normal">
            A class lists models to try in order; the next takes a request when an upstream fails.
            An agent, the teacher or a chat may name a class instead of a model.
          </p>
          {form.classes.map((each: ClassForm, place) => (
            <div
              key={each.uid}
              className="grid items-start gap-plusfifty md:grid-cols-[1fr_3.5fr_auto]"
              data-testid="class"
            >
              <Field label="Class">
                {(id) => (
                  <TextInput
                    id={id}
                    value={each.name}
                    placeholder="strong"
                    onChange={(event) => patchClass(place, { name: event.target.value })}
                  />
                )}
              </Field>
              <Field label="Models, first tried first">
                {(id) => (
                  <TextInput
                    id={id}
                    mono
                    value={each.models}
                    placeholder="claude-opus-5, qwen-local"
                    onChange={(event) => patchClass(place, { models: event.target.value })}
                  />
                )}
              </Field>
              <div className="md:pt-[22px]">
                <Button
                  variant="outline"
                  size="sm"
                  aria-label={`Remove class ${place + 1}`}
                  onClick={() => edit((next) => void next.classes.splice(place, 1))}
                >
                  Remove
                </Button>
              </div>
            </div>
          ))}
          <div>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                edit(
                  (next) =>
                    void next.classes.push({ uid: nextUid(), name: "", models: "", dayTokens: [] }),
                )
              }
            >
              Add class
            </Button>
          </div>
        </Box>

        <Box title="Agents">
          {data.agents.length === 0 ? (
            <p className="m-0 text-sm text-normal">The server has no agent.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b border-line text-xs text-low">
                    <th className="py-1 pr-base font-semibold">Agent</th>
                    <th className="py-1 pr-base font-semibold">Model</th>
                    <th className="py-1 pr-base font-semibold">Protocol</th>
                    <th className="py-1 pr-base font-semibold">Thinking</th>
                    <th className="py-1 font-semibold">Budget</th>
                  </tr>
                </thead>
                <tbody>
                  {data.agents.map((agent) => (
                    <tr key={agent.name} className="border-b border-line/50">
                      <td className="py-1.5 pr-base font-semibold text-high">{agent.name}</td>
                      <td className="py-1.5 pr-base text-normal">
                        {agent.error ? "Cannot run" : agent.model}
                      </td>
                      <td className="py-1.5 pr-base text-normal">
                        {agent.error ? "" : agent.protocol}
                      </td>
                      <td className="py-1.5 pr-base text-normal">{agent.thinking ?? "–"}</td>
                      <td className="py-1.5 text-normal">
                        {agent.token_budget === undefined ? "–" : tokens(agent.token_budget)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <p className="m-0 text-sm text-normal">Agents are edited on the Agents page.</p>
        </Box>

        <div className="grid items-start gap-double lg:grid-cols-2">
          <OptionalBox
            heading="Teacher"
            about="The teacher model checks the rules written in prose."
            off="No teacher is configured; a call is refused while a rule in prose applies."
            optional={form.teacher}
            choices={teacherChoices}
            note={note(true)}
            onToggle={() =>
              edit((next) => void Object.assign(next.teacher, { on: !next.teacher.on }))
            }
            onField={(key, text) => patchOptional("teacher", key, text)}
            onPick={(choice) => {
              patchOptional("teacher", "teacher.model", choice.id);
              // The teacher's protocol follows its model.
              if ((PROTOCOLS as readonly string[]).includes(choice.protocol)) {
                patchOptional("teacher", "teacher.protocol", choice.protocol);
              }
            }}
          />
          <OptionalBox
            heading="Decision model"
            about="The decision model shadows the teacher and can take kinds of decision over."
            off="No decision model is configured."
            optional={form.decisionModel}
            choices={decisionModelChoices}
            note={note(!form.teacher.on)}
            secrets={secrets}
            onToggle={() =>
              edit((next) => void Object.assign(next.decisionModel, { on: !next.decisionModel.on }))
            }
            onField={(key, text) => patchOptional("decisionModel", key, text)}
            onPick={(choice) => patchOptional("decisionModel", "decision_model.model", choice.id)}
            onKey={(change) =>
              edit((next) => void Object.assign(next.decisionModel.key?.now ?? {}, change))
            }
          />
        </div>
      </div>
    </>
  );
}

function Failed({ message }: { message: string }) {
  return (
    <>
      <Header breadcrumb="Setup" title="Models" />
      <Notice tone="error">{message}</Notice>
    </>
  );
}

/** A token count as the board writes it: 2.0M, 350k. */
function tokens(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1000) return `${Math.round(count / 1000)}k`;
  return String(count);
}

/** The amber line that says the server has to restart, with the button that restarts it. */
function Banner({ children, action }: { children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div
      role="status"
      className="flex items-center justify-between gap-plusfifty rounded-lg border border-amber-border bg-amber-fill px-double py-plusfifty font-mono text-[12.5px] text-amber"
    >
      <span className="min-w-0">{children}</span>
      {action}
    </div>
  );
}

/** Where an API key comes from: no key, an environment variable, or a key typed here and sent once. */
function KeyField({
  label,
  choice,
  secrets,
  onChange,
}: {
  label: string;
  choice: KeyChoice;
  secrets: string[];
  onChange: (change: Partial<KeyChoice>) => void;
}) {
  const stored = choice.stored !== null && secrets.includes(choice.stored);
  return (
    <div className="grid gap-plusfifty sm:grid-cols-2">
      <Field label="Key">
        {(id) => (
          <Select
            id={id}
            aria-label={`${label}: where it comes from`}
            options={KEY_MODES}
            value={choice.mode}
            onChange={(mode) => onChange({ mode: mode as KeyMode })}
          />
        )}
      </Field>
      <Field label=" ">
        {(id) =>
          choice.mode === "env" ? (
            <TextInput
              id={id}
              mono
              aria-label={`${label}: environment variable`}
              value={choice.env}
              placeholder="ANTHROPIC_API_KEY"
              onChange={(event) => onChange({ env: event.target.value })}
            />
          ) : choice.mode === "typed" ? (
            <TextInput
              id={id}
              type="password"
              autoComplete="new-password"
              aria-label={`${label}: the key`}
              value={choice.typed}
              placeholder={stored ? "Stored; type to replace it" : "Paste the key"}
              onChange={(event) => onChange({ typed: event.target.value })}
            />
          ) : (
            <span />
          )
        }
      </Field>
    </div>
  );
}

/** The teacher or the decision model: a box that can be turned on and off. */
function OptionalBox({
  heading,
  about,
  off,
  optional,
  choices,
  note,
  secrets,
  onToggle,
  onField,
  onPick,
  onKey,
}: {
  heading: string;
  about: string;
  off: string;
  optional: OptionalForm;
  choices: ReturnType<typeof modelChoices>;
  note: React.ReactNode;
  secrets?: string[];
  onToggle: () => void;
  onField: (key: string, text: string) => void;
  onPick: (choice: ReturnType<typeof modelChoices>[number]) => void;
  onKey?: (change: Partial<KeyChoice>) => void;
}) {
  if (!optional.on) {
    return (
      <Box title={heading}>
        <p className="m-0 text-sm text-normal">
          {optional.wasOn ? "Turned off; Save takes it out of the file." : off}
        </p>
        <div>
          <Button
            variant="outline"
            size="sm"
            aria-label={`Turn on the ${heading.toLowerCase()}`}
            onClick={onToggle}
          >
            Turn on
          </Button>
        </div>
      </Box>
    );
  }
  return (
    <Box title={heading}>
      <p className="m-0 text-sm text-normal">{about}</p>
      {optional.fields.map((field: FieldForm) => (
        <Field
          key={field.key}
          label={field.label}
          hint={
            field.default !== null && field.text.trim() === ""
              ? `Default ${field.default}`
              : undefined
          }
        >
          {(id) =>
            field.kind === "model" ? (
              <>
                <ModelField
                  id={id}
                  choices={choices}
                  value={field.text}
                  placeholder={field.default ?? ""}
                  onChange={(text) => onField(field.key, text)}
                  onPick={onPick}
                />
                {note}
              </>
            ) : field.kind === "pick" ? (
              <Select
                id={id}
                options={[...PROTOCOLS]}
                value={field.text}
                onChange={(text) => onField(field.key, text)}
              />
            ) : (
              <TextInput
                id={id}
                mono={field.kind === "code"}
                value={field.text}
                placeholder={field.default ?? ""}
                onChange={(event) => onField(field.key, event.target.value)}
              />
            )
          }
        </Field>
      ))}
      {optional.key && onKey && (
        <KeyField
          label="Key of the decision model"
          choice={optional.key.now}
          secrets={secrets ?? []}
          onChange={onKey}
        />
      )}
      <div>
        <Button
          variant="outline"
          size="sm"
          aria-label={`Turn off the ${heading.toLowerCase()}`}
          onClick={onToggle}
        >
          Turn off
        </Button>
      </div>
    </Box>
  );
}
