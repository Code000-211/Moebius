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
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Empty } from "../../ui/page/Empty";
import { FieldRow, Rows } from "../../ui/page/Field";
import { CELL, Table } from "../../ui/page/List";
import { Notice } from "../../ui/page/Notice";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { decisionChoices, modelChoices } from "../agents/model";
import { Answered } from "../harness/ModelField";
import { AreaTitle, Section } from "../settings/parts";
import type { LoginStep } from "./LoginRow";
import {
  blankUpstream,
  type ClassForm,
  type KeyChoice,
  type ModelsForm,
  modelsChanges,
  modelsDirty,
  nextUid,
  PROTOCOLS,
  readForm,
  type UpstreamForm,
} from "./model";
import { OptionalBox } from "./OptionalBox";
import { ProviderPanel } from "./ProviderPanel";
import { Failure, UpstreamRow } from "./UpstreamRow";

type Notice_ = { kind: "saved" } | { kind: "same" } | { kind: "failed"; text: string };

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
        <AreaTitle title="Models" />
        <SkeletonCard lines={6} message="Reading the models" />
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
      <AreaTitle
        title="Models"
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
      <div className="flex flex-col">
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
          <Section title="Providers">
            <Empty
              plain
              title="No provider"
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
          </Section>
        ) : (
          <Section
            title="Providers"
            aside={
              <span className="flex items-center gap-2">
                {answered && <Answered />}
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={refreshing}
                  onClick={() => void refresh()}
                >
                  {refreshing ? "Asking" : "Refresh"}
                </Button>
              </span>
            }
          >
            <Rows>
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
                <div key={error.upstream} className="py-3">
                  <Failure {...error} />
                </div>
              ))}
              <div className="flex flex-wrap gap-3 py-3">
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
            </Rows>
          </Section>
        )}
        {choosing && (
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
        )}

        <Section title="Model classes">
          <p className="m-0 mb-3 max-w-prose text-sm text-secondary">
            A class lists models to try in order; the next takes a request when one fails. An agent,
            the teacher or a chat may name a class instead of a model.
          </p>
          <Rows>
            {form.classes.map((each: ClassForm, place) => (
              <div key={each.uid} data-testid="class" className="divide-y divide-hairline">
                <FieldRow label="Class" wide>
                  <Input
                    value={each.name}
                    placeholder="strong"
                    onChange={(event) => patchClass(place, { name: event.target.value })}
                  />
                </FieldRow>
                <FieldRow
                  label="Models, first tried first"
                  description="Model ids or other classes, separated by commas."
                  wide
                >
                  <Input
                    mono
                    value={each.models}
                    placeholder="claude-opus-5, qwen-local"
                    onChange={(event) => patchClass(place, { models: event.target.value })}
                  />
                </FieldRow>
                <FieldRow label={`Class ${place + 1}`}>
                  <Button
                    variant="danger"
                    size="sm"
                    aria-label={`Remove class ${place + 1}`}
                    onClick={() => edit((next) => void next.classes.splice(place, 1))}
                  >
                    Remove
                  </Button>
                </FieldRow>
              </div>
            ))}
            <div className="py-3">
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
          </Rows>
        </Section>

        {data.agents.length === 0 ? (
          <Section title="Agents">
            <Empty
              plain
              title="No agent yet"
              body="Nothing asks a model until an agent exists. Create one on the Agents page."
              action={
                <Button asChild variant="secondary" size="sm">
                  <Link to="/agents">New agent</Link>
                </Button>
              }
            />
          </Section>
        ) : (
          <Section title="Agents">
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
            <p className="m-0 mt-3 text-sm text-secondary">
              Agents are edited on the{" "}
              <Link className="underline underline-offset-4" to="/agents">
                Agents page
              </Link>
              .
            </p>
          </Section>
        )}

        <div className="flex flex-col">
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
      <AreaTitle title="Models" />
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
