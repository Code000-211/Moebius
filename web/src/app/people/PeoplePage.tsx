/**
 * The People page, for owners: add a person (handle, name, role) and hand over their first token, change a role,
 * disable or enable a person, and see, make and revoke their tokens with each one's last use. A member or a viewer is
 * shown the page of an address that is not one; the server refuses them in any case. A disabled person is signed out at
 * once by the server.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { addPerson, changePerson, people, type Role, tokensOf } from "../../api/calls/people";
import { getSession } from "../../api/calls/session";
import { keys } from "../../api/keys";
import type { AddedPerson, Person } from "../../api/types/server";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { useErrorToast } from "../../ui/lib/useErrorToast";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { FieldRow, Rows, Select } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { AreaTitle, Section } from "../settings/parts";
import { NotFoundPage } from "../shell/Page";
import { useRole } from "../shell/session";
import { IssuedNotice, TokensList } from "./TokensList";

const ROLES: Role[] = ["owner", "member", "viewer"];

export function PeoplePage() {
  const role = useRole();
  if (role !== undefined && role !== "owner") return <NotFoundPage />;
  return <People />;
}

function People() {
  const list = useQuery({ queryKey: keys.people, queryFn: people });
  const [added, setAdded] = useState<AddedPerson | null>(null);
  return (
    <div data-testid="people-page" className="max-w-3xl">
      <AreaTitle title="Members" />
      <Section title="Add a person">
        <AddPerson onAdded={setAdded} />
        {added && (
          <div className="mt-3">
            <IssuedNotice issued={added.token} />
          </div>
        )}
      </Section>
      <Section title="Everyone">
        {list.isPending && <SkeletonLines lines={3} message="Reading the people" />}
        {list.isError && (
          <ReadFailed
            error={list.error}
            fallback="Could not read the people. Try again."
            busy={list.isFetching}
            onRetry={() => void list.refetch()}
          />
        )}
        {list.data && (
          <ul className="m-0 list-none border-t border-hairline p-0">
            {list.data.map((person) => (
              <PersonRow key={person.id} person={person} />
            ))}
          </ul>
        )}
      </Section>
    </div>
  );
}

function AddPerson({ onAdded }: { onAdded: (added: AddedPerson) => void }) {
  const queryClient = useQueryClient();
  const [handle, setHandle] = useState("");
  const [name, setName] = useState("");
  const [role, setRole] = useState<Role>("member");
  const add = useMutation({
    mutationFn: addPerson,
    onSuccess: async (made) => {
      onAdded(made);
      setHandle("");
      setName("");
      await queryClient.invalidateQueries({ queryKey: keys.people });
    },
  });
  useErrorToast(add.error, "Could not add the person.");
  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault();
        add.mutate({ handle, name, role });
      }}
    >
      <Rows>
        <FieldRow wide label="Handle" description="Lowercase letters, digits, - and _.">
          <Input value={handle} onChange={(event) => setHandle(event.target.value)} />
        </FieldRow>
        <FieldRow wide label="Name">
          <Input value={name} onChange={(event) => setName(event.target.value)} />
        </FieldRow>
        <FieldRow wide label="Role">
          <Select
            aria-label="Role"
            options={ROLES}
            value={role}
            onChange={(value) => setRole(value as Role)}
          />
        </FieldRow>
      </Rows>
      <Button
        type="submit"
        size="sm"
        className="self-start"
        disabled={handle.trim() === "" || name.trim() === "" || add.isPending}
      >
        Add person
      </Button>
    </form>
  );
}

function PersonRow({ person }: { person: Person }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [asking, setAsking] = useState(false);
  const me = useQuery({ queryKey: keys.session, queryFn: getSession, enabled: false }).data?.person;
  const change = useMutation({
    mutationFn: (what: { role?: Role; disabled?: boolean }) => changePerson(person.id, what),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.people }),
  });
  useErrorToast(change.error, "Could not change the person.");
  return (
    <li data-testid={`person-${person.handle}`} className="border-b border-hairline">
      <div className="flex min-h-9 flex-wrap items-center gap-2.5 py-1">
        <span className="min-w-0 flex-1 truncate text-base text-text">
          <span className="font-medium">{person.name}</span>{" "}
          <span className="text-muted">@{person.handle}</span>
          {person.disabled && <span className="ml-2 text-xs text-muted">Disabled</span>}
        </span>
        <Select
          aria-label={`Role of @${person.handle}`}
          className="w-32"
          options={ROLES}
          value={person.role}
          onChange={(value) => change.mutate({ role: value as Role })}
        />
        <Button
          variant="secondary"
          size="sm"
          onClick={() =>
            person.id === me?.id && !person.disabled
              ? setAsking(true)
              : change.mutate({ disabled: !person.disabled })
          }
        >
          {person.disabled ? "Enable" : "Disable"}
        </Button>
        <Button variant="ghost" size="sm" aria-expanded={open} onClick={() => setOpen(!open)}>
          Tokens
        </Button>
      </div>
      <ConfirmDelete
        open={asking}
        onOpenChange={setAsking}
        title="Disable yourself?"
        confirmLabel="Disable me"
        pendingLabel="Disabling"
        onConfirm={() => change.mutateAsync({ disabled: true }).then(() => undefined)}
      >
        This is your own row: you are signed out at once and cannot sign in again until another
        owner enables you.
      </ConfirmDelete>
      {open && (
        <div className="mt-2">
          <TokensList
            queryKey={keys.tokensOf(person.id)}
            read={() => tokensOf(person.id)}
            testId={`tokens-${person.handle}`}
          />
        </div>
      )}
    </li>
  );
}
