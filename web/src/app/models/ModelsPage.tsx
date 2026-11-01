/**
 * The Models page: where model requests go (the router's upstreams with their API keys, and the logins of those that
 * use OAuth), the model classes, the agents' models (read only; agents are edited on the Agents page), and the
 * teacher and the decision model, which may be left out. It edits `moebius.toml` through `PUT /v1/settings`: the form
 * is made of the keys `GET /v1/settings` tells and Save sends only what was changed (`model.ts`). An API key typed
 * here goes into a password field, goes to `PUT /v1/secrets/{name}` once, and is never read back: the field is empty
 * again after the save and says only that a key is stored. Each upstream answers in a word whether it is configured
 * and whether it served its models; one that did not says what failed and what to do. A key the running server uses
 * only after a restart makes a banner with a Restart button.
 */
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import {
  availableModels,
  models as readModels,
  providers as readProviders,
  restartServer,
  saveSettings,
  serverSettings,
  storeSecret,
} from "../../api/calls/models";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { AvailableModels } from "../../api/types/chat";
import type { UpstreamOAuth } from "../../api/types/models";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Loader } from "../../ui/components/Loader";
import { plural } from "../../ui/lib/format";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { Field, Select } from "../../ui/page/Field";
import { CELL, Table } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { Status } from "../../ui/page/Status";
import { decisionChoices, modelChoices } from "../agents/model";
import { Answered, couldNotAsk, ModelField } from "../harness/ModelField";
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

/** An upstream that did not answer, as `GET /v1/models/available` reported it. */
type AvailableError = AvailableModels["errors"][number];

/** An upstream that did not answer: what the server said, and what to do about it. */
function Failure({ upstream, message }: AvailableError) {
  return (
    <div className="flex flex-col gap-1">
      <span className="break-words text-sm text-error-text">{couldNotAsk(upstream, message)}</span>
      <span className="text-sm text-secondary">Check the address and the key, then refresh.</span>
    </div>
  );
}

/** The models an upstream served, so a person can see that it works without opening anything. */
const servedBy = (available: AvailableModels | undefined, baseUrl: string) =>
  available?.models.filter((model) => model.upstream === baseUrl).length ?? 0;

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
  // Whether the last ask of the upstreams answered with nothing wrong; it says nothing before the button is pressed.
  const [answered, setAnswered] = useState(false);
  const [restartedAt, setRestartedAt] = useState<number | null>(null);

  // What is being edited is kept when the server tells the same again; the form is made anew when nothing was changed.
  const updated = server.dataUpdatedAt;
  const read = server.data;
  useEffect(() => {
    if (!read) return;
    setForm((held) => (held === null || !modelsDirty(held) ? readForm(read) : held));
  }, [read]);

  if (models.isError)
    return <Failed message={sayError(models.error, "The models could not be read.")} />;
  if (server.isError)
    return <Failed message={sayError(server.error, "The server's own setup could not be read.")} />;
  if (models.isPending || server.isPending || form === null) {
    return (
      <>
        <PageHeader title="Models" />
        <Loader message="Reading the models" className="py-8" />
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
      setAddProblem(error instanceof Error ? error.message : "cannot add the provider");
    }
    setAdding(false);
  }

  async function refresh() {
    setRefreshing(true);
    setAnswered(false);
    try {
      const fresh = await availableModels(true, true);
      queryClient.setQueryData(keys.availableDecision, fresh);
      setAnswered(fresh.errors.length === 0);
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
  // Which upstreams did not answer, by the address the file has for them; the rest are said by their own row.
  const failures = new Map(available.data?.errors.map((error) => [error.upstream, error]) ?? []);
  const addresses = new Set(form.upstreams.map((upstream) => upstream.baseUrl));
  const unplaced = (available.data?.errors ?? []).filter((error) => !addresses.has(error.upstream));
  const asked =
    (available.data?.errors.length ?? 0) > 0 || (available.data?.models.length ?? 0) > 0;

  return (
    <>
      <PageHeader
        title="Models"
        context="Where model requests go."
        actions={
          <div className="flex items-center gap-3">
            {saving || !modelsDirty(form) ? null : (
              <span className="text-sm text-secondary">Unsaved changes</span>
            )}
            <Button size="sm" disabled={saving || !modelsDirty(form)} onClick={() => void save()}>
              {saving ? "Saving" : "Save"}
            </Button>
          </div>
        }
      />
      <div className="flex flex-col gap-8">
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

        {form.upstreams.length === 0 ? (
          <Empty
            title="Providers"
            body="Every model request fails until a provider has an address and a key — one that speaks OpenAI or Anthropic."
            action={
              <div className="flex flex-wrap gap-3">
                <Button
                  size="sm"
                  onClick={() => edit((next) => void next.upstreams.push(blankUpstream()))}
                >
                  Add provider
                </Button>
                <Button variant="secondary" size="sm" onClick={() => setChoosing(true)}>
                  Log in to a provider
                </Button>
              </div>
            }
          />
        ) : (
          <Box
            title="Providers"
            actions={
              <div className="flex items-center gap-2">
                {answered && <Answered />}
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={refreshing}
                  onClick={() => void refresh()}
                >
                  {refreshing ? "Asking" : "Refresh"}
                </Button>
              </div>
            }
            className="flex flex-col gap-4"
          >
            {form.upstreams.map((upstream, place) => {
              const oauth =
                upstream.origin === null ? undefined : data.upstreams[upstream.origin]?.oauth;
              return (
                <UpstreamRow
                  key={upstream.uid}
                  upstream={upstream}
                  place={place}
                  oauth={oauth}
                  step={upstream.origin === null ? undefined : logins[upstream.origin]}
                  failure={failures.get(upstream.baseUrl)}
                  served={servedBy(available.data, upstream.baseUrl)}
                  asked={asked}
                  secrets={secrets}
                  onPatch={(patch) => patchUpstream(place, patch)}
                  onKey={(change) => patchKey(place, change)}
                  onStep={(step) =>
                    setLogins((held) => {
                      const origin = upstream.origin;
                      if (origin === null) return held;
                      const next = { ...held };
                      if (step === undefined) delete next[origin];
                      else next[origin] = step;
                      return next;
                    })
                  }
                  onRemove={() => edit((next) => void next.upstreams.splice(place, 1))}
                />
              );
            })}
            {unplaced.map((error) => (
              <Failure key={error.upstream} {...error} />
            ))}
            <div className="flex flex-wrap gap-3">
              <Button
                variant="secondary"
                size="sm"
                onClick={() => edit((next) => void next.upstreams.push(blankUpstream()))}
              >
                Add provider
              </Button>
              <Button variant="secondary" size="sm" onClick={() => setChoosing(true)}>
                Log in to a provider
              </Button>
            </div>
            {choosing && (
              <div className="rounded-card bg-sunken p-4">
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
        )}

        <Box title="Model classes" className="flex flex-col gap-6">
          <p className="m-0 max-w-prose text-base text-secondary">
            A class lists models to try in order; the next takes a request when one fails. An agent,
            the teacher or a chat may name a class instead of a model.
          </p>
          {form.classes.map((each: ClassForm, place) => (
            <div key={each.uid} className="flex flex-col gap-4" data-testid="class">
              <div className="grid items-start gap-4 sm:grid-cols-2">
                <Field label="Class">
                  {(id) => (
                    <Input
                      id={id}
                      value={each.name}
                      placeholder="strong"
                      onChange={(event) => patchClass(place, { name: event.target.value })}
                    />
                  )}
                </Field>
                <Field
                  label="Models, first tried first"
                  hint="Model ids or other classes, separated by commas."
                >
                  {(id) => (
                    <Input
                      id={id}
                      mono
                      value={each.models}
                      placeholder="claude-opus-5, qwen-local"
                      onChange={(event) => patchClass(place, { models: event.target.value })}
                    />
                  )}
                </Field>
              </div>
              <div className="flex justify-end">
                <Button
                  variant="danger"
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
              variant="secondary"
              size="sm"
              onClick={() =>
                edit(
                  (next) =>
                    void next.classes.push({
                      uid: nextUid(),
                      name: "",
                      models: "",
                      dayTokens: [],
                    }),
                )
              }
            >
              Add class
            </Button>
          </div>
        </Box>

        {data.agents.length === 0 ? (
          <Empty
            title="No agent yet"
            body="Nothing asks a model until an agent exists. Create one on the Agents page."
            action={
              <Button asChild variant="secondary" size="sm">
                <Link to="/agents">New agent</Link>
              </Button>
            }
          />
        ) : (
          <Box title="Agents" className="flex flex-col gap-4">
            <Table head={["Agent", "Model", "Protocol", "Thinking", "Budget"]}>
              {data.agents.map((agent) => (
                <tr key={agent.name}>
                  <td className={CELL}>
                    <span className="font-semibold text-text">{agent.name}</span>
                  </td>
                  <td className={`${CELL} break-words`}>
                    {agent.error ? (
                      <span className="text-error-text">Cannot run</span>
                    ) : (
                      <span className="font-mono text-sm">{agent.model}</span>
                    )}
                  </td>
                  <td className={CELL}>{agent.error ? "" : agent.protocol}</td>
                  <td className={CELL}>{agent.thinking ?? "–"}</td>
                  <td className={`${CELL} tabular-nums`}>
                    {agent.token_budget === undefined ? "–" : tokens(agent.token_budget)}
                  </td>
                </tr>
              ))}
            </Table>
            <p className="m-0 text-sm text-secondary">
              Agents are edited on the{" "}
              <Link className="underline underline-offset-4" to="/agents">
                Agents page
              </Link>
              .
            </p>
          </Box>
        )}

        <div className="grid items-start gap-8 lg:grid-cols-2">
          <OptionalBox
            heading="Teacher"
            about="The teacher model checks the rules written in prose."
            off="No teacher is configured; a call is refused while a rule in prose applies."
            optional={form.teacher}
            choices={teacherChoices}
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

/** Why the page cannot be read at all. */
function Failed({ message }: { message: string }) {
  return (
    <>
      <PageHeader title="Models" />
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

/** The line that says the server has to restart, with the button that restarts it. */
function Banner({ children, action }: { children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div
      role="status"
      className="flex flex-wrap items-center justify-between gap-4 rounded-control bg-attention-fill px-4 py-3 text-sm font-semibold text-attention-text"
    >
      <span className="min-w-0 break-words">{children}</span>
      {action}
    </div>
  );
}

/**
 * One upstream, and the heart of the page: a word for whether it answers and a word for where its key comes from,
 * then the address, the protocol and the key. One that did not answer says what the server said and what to do, and
 * one that logs in carries its login under the fields.
 */
function UpstreamRow({
  upstream,
  place,
  oauth,
  step,
  failure,
  served,
  asked,
  secrets,
  onPatch,
  onKey,
  onStep,
  onRemove,
}: {
  upstream: UpstreamForm;
  place: number;
  oauth: UpstreamOAuth | null | undefined;
  step: LoginStep | undefined;
  failure: AvailableError | undefined;
  /** How many models this upstream served the last time the app asked. */
  served: number;
  /** Whether the app has asked the upstreams at all. */
  asked: boolean;
  secrets: string[];
  onPatch: (patch: Partial<UpstreamForm>) => void;
  onKey: (change: Partial<KeyChoice>) => void;
  onStep: (step: LoginStep | undefined) => void;
  onRemove: () => void;
}) {
  const needsAddress = upstream.baseUrl.trim() === "";
  const answer = failure
    ? { tone: "error" as const, text: "Did not answer" }
    : served > 0
      ? { tone: "complete" as const, text: plural(served, "model", "models") }
      : needsAddress
        ? { tone: "attention" as const, text: "Needs an address" }
        : asked
          ? { tone: "neutral" as const, text: "No models yet" }
          : { tone: "neutral" as const, text: "Not asked yet" };
  return (
    <section data-testid="upstream" className="flex flex-col gap-4 rounded-control bg-sunken p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <Status tone={answer.tone}>{answer.text}</Status>
          {oauth?.needs && <Status tone="attention">Needs {oauth.needs}</Status>}
        </div>
        <Button
          variant="danger"
          size="sm"
          aria-label={`Remove provider ${place + 1}`}
          onClick={onRemove}
        >
          Remove
        </Button>
      </div>
      <div className="grid items-start gap-4 sm:grid-cols-2">
        <Field label="Protocol">
          {(id) => (
            <Select
              id={id}
              aria-label="Protocol"
              options={[...PROTOCOLS]}
              value={upstream.protocol}
              onChange={(protocol) => onPatch({ protocol })}
            />
          )}
        </Field>
        <Field label="Base URL" hint="The address Moebius sends the requests to.">
          {(id) => (
            <Input
              id={id}
              mono
              aria-label="Base URL"
              value={upstream.baseUrl}
              placeholder="http://127.0.0.1:8080"
              onChange={(event) => onPatch({ baseUrl: event.target.value })}
            />
          )}
        </Field>
      </div>
      {oauth ? (
        // Not a `Field` with an id: there is no control here, so the label would point at nothing.
        <Field label="Key">
          <span className="py-2 text-sm text-secondary">None: it logs in with OAuth</span>
        </Field>
      ) : (
        <KeyField
          label={`Key of provider ${place + 1}`}
          choice={upstream.key}
          secrets={secrets}
          onChange={onKey}
        />
      )}
      {failure && <Failure upstream={failure.upstream} message={failure.message} />}
      {oauth && upstream.origin !== null && (
        <LoginRow place={upstream.origin} oauth={oauth} step={step} onStep={onStep} />
      )}
    </section>
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
    <div className="flex flex-col gap-4">
      <div className="grid items-start gap-4 sm:grid-cols-2">
        <Field label="Key" hint={keyHint(choice, stored)}>
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
        {choice.mode === "env" && (
          <Field
            label="Environment variable"
            hint="The server reads it from its own environment when a run starts."
          >
            {(id) => (
              <Input
                id={id}
                mono
                aria-label={`${label}: environment variable`}
                value={choice.env}
                placeholder="ANTHROPIC_API_KEY"
                onChange={(event) => onChange({ env: event.target.value })}
              />
            )}
          </Field>
        )}
        {choice.mode === "typed" && (
          <Field
            label="API key"
            hint={
              stored
                ? "Stored on the server by name. Type to replace it; it is never shown back."
                : "Sent once, then stored on the server by name and never shown back."
            }
          >
            {(id) => (
              <Input
                id={id}
                type="password"
                autoComplete="new-password"
                aria-label={`${label}: the key`}
                value={choice.typed}
                placeholder={stored ? "Stored; type to replace it" : "Paste the key"}
                onChange={(event) => onChange({ typed: event.target.value })}
              />
            )}
          </Field>
        )}
      </div>
      {choice.mode === "none" && (
        <p className="m-0 text-sm text-secondary">
          No key: the address is open, or a login below supplies it.
        </p>
      )}
    </div>
  );
}

/** Whether a key is there, in a word, under the choice of where it comes from. */
function keyHint(choice: KeyChoice, stored: boolean): string {
  if (choice.mode === "env")
    return choice.env.trim() === "" ? "Name the variable the server reads." : "Configured.";
  if (choice.mode === "typed") return stored ? "Configured." : "Needs a key.";
  return "No key.";
}

/** The teacher or the decision model: a box that can be turned on and off. */
function OptionalBox({
  heading,
  about,
  off,
  optional,
  choices,
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
  secrets?: string[];
  onToggle: () => void;
  onField: (key: string, text: string) => void;
  onPick: (choice: ReturnType<typeof modelChoices>[number]) => void;
  onKey?: (change: Partial<KeyChoice>) => void;
}) {
  if (!optional.on) {
    return (
      <Box title={heading} className="flex flex-col gap-4">
        <p className="m-0 max-w-prose text-base text-secondary">
          {optional.wasOn ? "Turned off; Save takes it out of the file." : off}
        </p>
        <div>
          <Button
            variant="secondary"
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
    <Box title={heading} className="flex flex-col gap-4">
      <p className="m-0 max-w-prose text-base text-secondary">{about}</p>
      <div className="grid items-start gap-4 sm:grid-cols-2">
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
                <ModelField
                  id={id}
                  choices={choices}
                  value={field.text}
                  placeholder={field.default ?? ""}
                  onChange={(text) => onField(field.key, text)}
                  onPick={onPick}
                />
              ) : field.kind === "pick" ? (
                <Select
                  id={id}
                  options={[...PROTOCOLS]}
                  value={field.text}
                  onChange={(text) => onField(field.key, text)}
                />
              ) : (
                <Input
                  id={id}
                  mono={field.kind === "code" || field.kind === "list"}
                  className={
                    field.kind === "whole" || field.kind === "decimal" ? "tabular-nums" : undefined
                  }
                  value={field.text}
                  placeholder={field.default ?? ""}
                  onChange={(event) => onField(field.key, event.target.value)}
                />
              )
            }
          </Field>
        ))}
      </div>
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
          variant="secondary"
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
