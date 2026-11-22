/**
 * The Rules page: a sub-navigation at the left (the same look as Settings, `app/settings/SubNav.tsx`) with the sets
 * of rules, and at the right the rules of the chosen one as blocks (`RuleList`). The sets are the built-in patterns
 * (always on, read only), the server's rules in prose and one entry for each repository, each with its count, then
 * "Refusals", the rules that refused a call this week (`scopes.ts`). The chosen set is the `scope` of the address.
 * "New rule" asks for the scope. The server's rules are saved
 * with `PUT /v1/rules`, a repository's with `PUT /v1/repositories/{id}/settings/rules.md`; the server checks both and
 * says which line is wrong.
 */

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { rules, saveRepositoryRules, saveRules } from "../../api/calls/rules";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { RulesView } from "../../api/types/rules";
import { Button } from "../../ui/components/Button";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { FolderIcon, LockIcon, ProhibitIcon, ServerIcon } from "../../ui/icons";
import { cn } from "../../ui/lib/cn";
import { plural } from "../../ui/lib/format";
import { Notice } from "../../ui/page/Notice";
import { InSettingsArea, PageHeader } from "../../ui/page/PageHeader";
import { Reading, SkeletonCard } from "../../ui/page/Skeleton";
import { SUB_NAV_GROUP, SubNavLabel, subNavRow } from "../settings/SubNav";
import { NewRule } from "./NewRule";
import { RuleBlock, RuleList, refused, ScopeChip } from "./RuleList";
import { chosenScope, type Scope, scopeKey, scopeName } from "./scopes";

export function RulesPage() {
  const queryClient = useQueryClient();
  const data = useQuery({ queryKey: keys.rules, queryFn: rules });
  const refresh = () => queryClient.invalidateQueries({ queryKey: keys.rules });
  const [params, setQuery] = useQueryParams();
  const [choosing, setChoosing] = useState(false);

  if (data.isPending || data.error || !data.data) {
    return (
      <Frame header={<PageHeader title="Rules" />}>
        {data.isPending ? (
          <Reading message="Reading the rules" className="flex flex-col gap-6">
            <SkeletonCard lines={5} />
          </Reading>
        ) : (
          <Notice tone="error">
            {sayError(data.error, "Could not read the rules. Try again.")}
          </Notice>
        )}
      </Frame>
    );
  }
  const view = data.data;
  const scope = chosenScope(params, view);
  const here = scopeKey(scope);
  const creating = params.get("new") === "1";
  const canWriteServer = view.prose.editable;

  /** The scope "New rule" starts on: the one being read when its rules can be written, else the server's. */
  const defaultScope = (): string =>
    (scope.kind === "server" && canWriteServer) || scope.kind === "repository"
      ? here
      : canWriteServer
        ? "server"
        : view.repositories[0]
          ? scopeKey({ kind: "repository", id: view.repositories[0].repository_id })
          : "";
  const canCreate = canWriteServer || view.repositories.length > 0;

  const header = (
    <PageHeader
      title="Rules"
      primary={
        canCreate && (
          <Button size="sm" onClick={() => setChoosing(true)}>
            New rule
          </Button>
        )
      }
    />
  );

  return (
    <Frame
      header={header}
      nav={
        <nav
          aria-label="Rules"
          className="flex w-50 shrink-0 flex-col gap-3 overflow-y-auto border-r border-hairline px-2 py-2"
        >
          <div className="flex flex-col gap-0.5">
            <NavRow
              to={{ kind: "builtin" }}
              here={here}
              icon={LockIcon}
              label="Built-in"
              count={view.patterns.length}
            />
            <NavRow
              to={{ kind: "server" }}
              here={here}
              icon={ServerIcon}
              label="Server"
              count={view.prose.rules.length}
            />
          </div>
          {view.repositories.length > 0 && (
            <div className="flex flex-col gap-0.5">
              <span className={SUB_NAV_GROUP}>Repositories</span>
              {view.repositories.map((each) => (
                <NavRow
                  key={each.repository_id}
                  to={{ kind: "repository", id: each.repository_id }}
                  here={here}
                  icon={FolderIcon}
                  label={each.repository}
                  count={each.rules.length}
                />
              ))}
            </div>
          )}
          <div className="flex flex-col gap-0.5">
            <NavRow to={{ kind: "refusals" }} here={here} icon={ProhibitIcon} label="Refusals" />
          </div>
        </nav>
      }
    >
      <section aria-label={scopeName(scope, view)} className="max-w-170">
        <Scoped
          scope={scope}
          view={view}
          creating={creating}
          onCreated={() => setQuery({ new: "" })}
          onSave={{
            server: async (file) => {
              const saved = await saveRules(file);
              await refresh();
              return saved.commit;
            },
            repository: async (id, file) => {
              await saveRepositoryRules(id, file);
              await refresh();
              return undefined;
            },
          }}
        />
      </section>
      <NewRule
        open={choosing}
        onOpenChange={setChoosing}
        view={view}
        initial={defaultScope()}
        onChoose={(next) => {
          setChoosing(false);
          setQuery({ scope: next, new: "1" });
        }}
      />
    </Frame>
  );
}

/** The page's frame: the header across the top, the sub-navigation under it and the chosen area, which scrolls on its own. */
function Frame({
  header,
  nav,
  children,
}: {
  header: React.ReactNode;
  nav?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="-m-4 flex h-[calc(100%+2rem)] flex-col">
      <div className="shrink-0 px-4 pt-4 [&>header]:mb-0 [&>header]:static">{header}</div>
      <div className="flex min-h-0 flex-1 items-stretch">
        {nav}
        <div className="min-w-0 flex-1 overflow-y-auto px-10 pb-6 pt-5">
          <InSettingsArea.Provider value>{children}</InSettingsArea.Provider>
        </div>
      </div>
    </div>
  );
}

/** One row of the sub-navigation: the icon, the name and, at the right, how many rules the set has. */
function NavRow({
  to,
  here,
  icon,
  label,
  count,
}: {
  to: Scope;
  here: string;
  icon: React.ComponentProps<typeof SubNavLabel>["icon"];
  label: string;
  count?: number;
}) {
  const key = scopeKey(to);
  return (
    <Link
      to={`?scope=${key}`}
      replace
      aria-current={key === here ? "page" : undefined}
      className={cn(subNavRow(key === here), "justify-between")}
    >
      <span className="flex min-w-0 items-center gap-2.5">
        <SubNavLabel icon={icon} label={label} />
      </span>
      {count !== undefined && (
        <span
          className={cn(
            "shrink-0 text-sm tabular-nums",
            count === 0 ? "text-muted" : "text-secondary",
          )}
        >
          {count}
        </span>
      )}
    </Link>
  );
}

/** The rules of the chosen scope. */
function Scoped({
  scope,
  view,
  creating,
  onCreated,
  onSave,
}: {
  scope: Scope;
  view: RulesView;
  creating: boolean;
  onCreated: () => void;
  onSave: {
    server: (file: string) => Promise<string | undefined>;
    repository: (id: number, file: string) => Promise<string | undefined>;
  };
}) {
  const none = `No rules for ${scopeName(scope, view)}.`;
  if (scope.kind === "builtin") {
    return (
      <>
        {view.patterns.map((pattern) => (
          <RuleBlock
            key={pattern.name}
            name={pattern.name}
            text={pattern.description}
            chip="Built-in"
            meta={["Always on, in every run", refused(view.refusals, pattern.name)]}
          />
        ))}
      </>
    );
  }
  if (scope.kind === "server") {
    const prose = view.prose;
    return (
      <>
        {prose.error && <Notice tone="error">{prose.error}</Notice>}
        <RuleList
          rules={prose.rules}
          file={prose.text}
          refusals={view.refusals}
          editable={prose.editable}
          empty={none}
          chip="Server"
          where={prose.editable ? "Every repository" : "Every repository · read only"}
          creating={creating && prose.editable}
          onCreated={onCreated}
          onSave={onSave.server}
        />
      </>
    );
  }
  if (scope.kind === "repository") {
    const repository = view.repositories.find((each) => each.repository_id === scope.id);
    if (!repository) return null;
    return (
      <>
        {repository.error && <Notice tone="error">{repository.error}</Notice>}
        <RuleList
          rules={repository.rules}
          file={repository.text}
          refusals={view.refusals}
          editable
          empty={none}
          chip={repository.repository}
          where={`Runs in ${repository.repository}`}
          creating={creating}
          onCreated={onCreated}
          onSave={(file) => onSave.repository(repository.repository_id, file)}
        />
      </>
    );
  }
  return <Refusals view={view} />;
}

/** The rules that refused a call this week, the most refusing first; a rule that refused nothing is not here. */
function Refusals({ view }: { view: RulesView }) {
  const all = [
    ...view.patterns.map((each) => ({ name: each.name, chip: "Built-in" })),
    ...view.prose.rules.map((each) => ({ name: each.name, chip: "Server" })),
    ...view.repositories.flatMap((repository) =>
      repository.rules.map((each) => ({ name: each.name, chip: repository.repository })),
    ),
  ];
  const refusing = all
    .map((each) => ({ ...each, count: view.refusals[each.name] ?? 0 }))
    .filter((each) => each.count > 0)
    .sort((a, b) => b.count - a.count);
  if (refusing.length === 0)
    return <p className="m-0 text-base text-muted">No refusals this week.</p>;
  return (
    <ul className="m-0 flex list-none flex-col p-0">
      {refusing.map((each) => (
        <li
          key={`${each.chip}/${each.name}`}
          className="flex min-h-9 items-center gap-2 border-b border-hairline py-2 text-base"
        >
          <span className="min-w-0 truncate font-semibold text-text">{each.name}</span>
          <ScopeChip>{each.chip}</ScopeChip>
          <span className="ml-auto shrink-0 text-sm tabular-nums text-attention-text">
            {plural(each.count, "refusal", "refusals")}
          </span>
        </li>
      ))}
    </ul>
  );
}
