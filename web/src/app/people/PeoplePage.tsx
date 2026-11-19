/**
 * The People page, for owners: add a person (handle, name, role) and hand over their first token, change a role,
 * disable or enable a person, and see, make and revoke their tokens with each one's last use. A member or a viewer is
 * shown the page of an address that is not one; the server refuses them in any case. A disabled person is signed out at
 * once by the server.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { addPerson, changePerson, people, type Role, tokensOf } from "../../api/calls/people";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { AddedPerson, Person } from "../../api/types/server";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { SkeletonLines } from "../../ui/page/Skeleton";
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
    <div data-testid="people-page" className="flex flex-col gap-6">
      <PageHeader
        title="People"
        context="Who can sign in, with which role. A viewer reads and proposes, a member works on cards, an owner configures."
      />
      <AddPerson onAdded={setAdded} />
      {added && (
        <Box title={`@${added.person.handle} is added`}>
          <IssuedNotice issued={added.token} />
        </Box>
      )}
      <Box title="Everyone" padded={false}>
        {list.isPending && <SkeletonLines lines={3} message="Reading the people" />}
        {list.isError && (
          <Notice tone="error">
            {sayError(list.error, "Could not read the people. Try again.")}
          </Notice>
        )}
        {list.data && (
          <ul className="m-0 flex list-none flex-col p-0">
            {list.data.map((person) => (
              <PersonRow key={person.id} person={person} />
            ))}
          </ul>
        )}
      </Box>
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
  return (
    <Box title="Add a person">
      <form
        className="flex flex-col gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          add.mutate({ handle, name, role });
        }}
      >
        <div className="grid gap-3 sm:grid-cols-3">
          <Field label="Handle" hint="Lowercase letters, digits, - and _.">
            <Input value={handle} onChange={(event) => setHandle(event.target.value)} />
          </Field>
          <Field label="Name">
            <Input value={name} onChange={(event) => setName(event.target.value)} />
          </Field>
          <Field label="Role">
            {(id) => (
              <Select
                id={id}
                options={ROLES}
                value={role}
                onChange={(value) => setRole(value as Role)}
              />
            )}
          </Field>
        </div>
        {add.isError && (
          <Notice tone="error">
            {sayError(add.error, "Could not add the person. Try again.")}
          </Notice>
        )}
        <Button
          type="submit"
          disabled={handle.trim() === "" || name.trim() === "" || add.isPending}
        >
          Add person
        </Button>
      </form>
    </Box>
  );
}

function PersonRow({ person }: { person: Person }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const change = useMutation({
    mutationFn: (what: { role?: Role; disabled?: boolean }) => changePerson(person.id, what),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.people }),
  });
  return (
    <li
      data-testid={`person-${person.handle}`}
      className="border-b border-hairline px-4 py-2 last:border-b-0"
    >
      <div className="flex flex-wrap items-center gap-3">
        <span className="min-w-0 flex-1 break-words text-base text-text">
          <span className="font-semibold">{person.name}</span>{" "}
          <span className="text-secondary">@{person.handle}</span>
          {person.disabled && <span className="ml-2 text-sm text-secondary">Disabled</span>}
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
          onClick={() => change.mutate({ disabled: !person.disabled })}
        >
          {person.disabled ? "Enable" : "Disable"}
        </Button>
        <Button variant="ghost" size="sm" aria-expanded={open} onClick={() => setOpen(!open)}>
          Tokens
        </Button>
      </div>
      {change.isError && (
        <Notice tone="error" className="mt-2">
          {sayError(change.error, "Could not change the person. Try again.")}
        </Notice>
      )}
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
