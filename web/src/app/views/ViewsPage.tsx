/**
 * The Views page: the saved views of the person, their own and the shared ones, each a filter with a layout and a sort.
 * Opening one shows its cards, which the server works out for the person who opens it, so a shared view shows every
 * member the same filter within what they may see. The cards are drawn in the view's layout: a list, the lanes of a
 * board, a calendar by due date or a timeline by cycle. A view is made from the board ("Save view") and changed here:
 * its name, whether it is shared, its layout and its sort. Only its owner (or an owner of the organisation) changes a
 * shared view. A view of the board layout whose filter the board's filter bar can hold opens on that board instead,
 * with its filter set (as does a list); `?manage` keeps this page for it, to change or delete it.
 */
import { BookmarkSimpleIcon, PencilSimpleIcon, TrashIcon, UsersIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, Navigate, useNavigate, useParams, useSearchParams } from "react-router";
import { deleteView, editView, listViews, viewCards } from "../../api/calls/views";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { View, ViewFields, ViewLayout } from "../../api/types/views";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Input } from "../../ui/components/Input";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Empty } from "../../ui/page/Empty";
import { Field, Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { cardKey } from "../board/cardKey";
import { cardPath } from "../pages";
import { boardOfView, fieldsOf, filterWords, groupsOf, LAYOUTS, SORTS } from "./model";

export function ViewsPage() {
  const { id } = useParams();
  const selected = id === undefined ? null : Number(id);
  // `?manage` opens the page of a view that would open on its board: the board links here to change or delete it.
  const managing = useSearchParams()[0].has("manage");
  const views = useQuery({ queryKey: keys.views, queryFn: listViews });
  const [editing, setEditing] = useState<View | null>(null);
  const [deleting, setDeleting] = useState<View | null>(null);
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  if (views.isPending) {
    return (
      <>
        <PageHeader title="Views" />
        <SkeletonList rows={3} message="Reading the views" />
      </>
    );
  }
  if (views.isError) {
    return (
      <>
        <PageHeader title="Views" />
        <ReadFailed
          error={views.error}
          fallback="The views could not be read. Try again."
          busy={views.isFetching}
          onRetry={() => void views.refetch()}
        />
      </>
    );
  }
  const current = views.data.find((view) => view.id === selected) ?? null;
  // A view of the board or list layout that the board can set is opened on its board, with its filter in the bar.
  const board = current && !managing ? boardOfView(current) : null;
  if (board !== null) return <Navigate to={board} replace />;
  return (
    <div data-testid="views-page" className="flex flex-col gap-4">
      <PageHeader
        title="Views"
        context="A view is a filter with a layout and a sort. Make one with Save view on a board; share it to show every member the same filter within what each may see."
      />
      {views.data.length === 0 ? (
        <Empty
          title="No view yet"
          body="Set filters on a board, then choose Save view. A view you make is yours until you share it."
        />
      ) : (
        <ul className="m-0 flex list-none flex-wrap gap-2 p-0" aria-label="Views">
          {views.data.map((view) => (
            <li key={view.id}>
              <Link
                to={`/views/${view.id}`}
                aria-current={view.id === selected ? "page" : undefined}
                className={`inline-flex min-h-8 items-center gap-2 rounded-control px-3 text-sm font-medium text-text no-underline hover:bg-hover ${
                  view.id === selected ? "bg-selected" : "bg-raised shadow-card"
                }`}
              >
                <BookmarkSimpleIcon className="size-4 text-secondary" aria-hidden="true" />
                {view.name}
                {view.shared && (
                  <span className="inline-flex items-center gap-1 text-secondary">
                    <UsersIcon className="size-3.5" aria-hidden="true" />
                    Shared
                  </span>
                )}
              </Link>
            </li>
          ))}
        </ul>
      )}
      {selected !== null && current === null && (
        <Notice tone="error">
          That view is not there. It may be personal to someone else, or deleted.
        </Notice>
      )}
      {current && (
        <OpenView
          view={current}
          onEdit={() => setEditing(current)}
          onDelete={() => setDeleting(current)}
        />
      )}
      <Dialog open={editing !== null} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent>
          {editing && (
            <ViewForm
              key={editing.id}
              view={editing}
              onDone={() => {
                void queryClient.invalidateQueries({ queryKey: keys.views });
                setEditing(null);
              }}
              onCancel={() => setEditing(null)}
            />
          )}
        </DialogContent>
      </Dialog>
      {deleting && (
        <ConfirmDelete
          open
          onOpenChange={(open) => !open && setDeleting(null)}
          title={`Delete ${deleting.name}?`}
          onConfirm={async () => {
            await deleteView(deleting.id);
            await queryClient.invalidateQueries({ queryKey: keys.views });
            navigate("/views");
          }}
        >
          The view goes{deleting.shared ? " for everyone it was shared with" : ""}. Its cards stay,
          where they are.
        </ConfirmDelete>
      )}
    </div>
  );
}

/** One view: what it asks, its buttons, and the cards it shows this person in its layout. */
function OpenView({
  view,
  onEdit,
  onDelete,
}: {
  view: View;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const shown = useQuery({
    queryKey: [...keys.viewCards(view.id), view.layout, view.sort, view.descending, view.filter],
    queryFn: () => viewCards(view.id),
  });
  const groups = groupsOf(view.layout, shown.data?.cards ?? []);
  return (
    <section aria-label={view.name} className="flex flex-col gap-3" data-testid="view">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="m-0 text-lg font-semibold text-text">{view.name}</h2>
        <span className="text-sm text-secondary">
          {view.shared ? "Shared with everyone" : "Only you"} ·{" "}
          {LAYOUTS.find(([layout]) => layout === view.layout)?.[1]} · sorted by{" "}
          {SORTS.find(([sort]) => sort === view.sort)?.[1].toLowerCase()}
          {view.descending ? ", reversed" : ""}
        </span>
        <span className="flex-1" />
        {view.can_change && (
          <>
            <Button size="sm" variant="secondary" onClick={onEdit}>
              <PencilSimpleIcon className="size-4" aria-hidden="true" />
              Edit
            </Button>
            <Button size="sm" variant="ghost" onClick={onDelete}>
              <TrashIcon className="size-4" aria-hidden="true" />
              Delete
            </Button>
          </>
        )}
      </div>
      <p className="m-0 text-sm text-secondary" data-testid="view-filter">
        Shows {filterWords(view.filter)}
        {boardOfView(view) && (
          <>
            {" · "}
            <Link to={boardOfView(view) as string}>Open on the board</Link>
          </>
        )}
      </p>
      {shown.isPending ? (
        <SkeletonList rows={3} message="Reading the cards" />
      ) : shown.isError ? (
        <ReadFailed
          error={shown.error}
          fallback="The cards of this view could not be read. Try again."
          busy={shown.isFetching}
          onRetry={() => void shown.refetch()}
        />
      ) : shown.data.cards.length === 0 ? (
        <Empty title="No card" body="No card you may see passes this view's filter." />
      ) : (
        <div
          className={view.layout === "board" ? "flex gap-3 overflow-x-auto" : "flex flex-col gap-3"}
        >
          {groups.map((group) => (
            <div
              key={group.key}
              data-testid="view-group"
              className={view.layout === "board" ? "min-w-64 flex-1" : ""}
            >
              {group.name !== "" && (
                <h3 className="m-0 mb-1 text-sm font-semibold text-secondary">
                  {group.name} <span className="font-normal">{group.cards.length}</span>
                </h3>
              )}
              <ul className="m-0 list-none rounded-card bg-raised p-0 shadow-card">
                {group.cards.map((card) => (
                  <li
                    key={card.id}
                    data-testid={`view-card-${card.id}`}
                    className="flex flex-wrap items-baseline gap-x-3 border-0 border-b border-solid border-hairline px-4 py-2 last:border-b-0"
                  >
                    <span className="text-sm text-secondary tabular-nums">{cardKey(card)}</span>
                    <Link
                      className="min-w-0 flex-1 text-text underline decoration-hairline-strong underline-offset-4 hover:decoration-current"
                      to={cardPath(card.repository_id, card.id)}
                    >
                      {card.title}
                    </Link>
                    {view.layout !== "board" && (
                      <span className="text-sm text-secondary">{card.column}</span>
                    )}
                    {card.due && view.layout !== "calendar" && (
                      <span className="text-sm text-secondary">due {card.due}</span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/** The dialog that changes a view's name, sharing, layout and sort. Its filter is kept as it is. */
function ViewForm({
  view,
  onDone,
  onCancel,
}: {
  view: View;
  onDone: () => void;
  onCancel: () => void;
}) {
  const [fields, setFields] = useState<ViewFields>(fieldsOf(view));
  const save = useMutation({
    // The filter is not sent, so the server keeps it as it is, with ids this person may not see.
    mutationFn: () => {
      const { filter: _kept, ...changed } = fields;
      return editView(view.id, { ...changed, name: fields.name.trim() });
    },
    onSuccess: onDone,
  });
  const ready = fields.name.trim() !== "" && !save.isPending;
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) save.mutate();
      }}
    >
      <DialogHeader>
        <DialogTitle>Edit view</DialogTitle>
        <DialogDescription>
          A shared view shows every member the same filter, within the projects each may see.
        </DialogDescription>
      </DialogHeader>
      <Field label="Name">
        <Input
          value={fields.name}
          autoComplete="off"
          onChange={(event) => setFields({ ...fields, name: event.target.value })}
        />
      </Field>
      <Field label="Layout">
        <Select
          aria-label="Layout"
          options={LAYOUTS}
          value={fields.layout}
          onChange={(layout) => setFields({ ...fields, layout: layout as ViewLayout })}
        />
      </Field>
      <Field label="Sort">
        <Select
          aria-label="Sort"
          options={SORTS}
          value={fields.sort}
          onChange={(sort) => setFields({ ...fields, sort })}
        />
      </Field>
      <label className="flex items-center gap-2 text-sm text-text">
        <input
          type="checkbox"
          checked={fields.descending}
          onChange={(event) => setFields({ ...fields, descending: event.target.checked })}
        />
        Reverse the order
      </label>
      <label className="flex items-center gap-2 text-sm text-text">
        <input
          type="checkbox"
          checked={fields.shared}
          onChange={(event) => setFields({ ...fields, shared: event.target.checked })}
        />
        Share with every member
      </label>
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save the view. </span>
          {sayError(save.error, "Try again.")}
        </Notice>
      )}
      <DialogFooter>
        <Button type="button" variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={!ready}>
          {save.isPending ? "Saving" : "Save"}
        </Button>
      </DialogFooter>
    </form>
  );
}
