/**
 * The Lessons area of Settings: what the Retro learned from earlier cards (`GET /v1/lessons`), each with its
 * kind, the cards it comes from and how often a run was given it. A person edits a lesson in place
 * (`PUT /v1/lessons/{id}`) and deletes it (`DELETE /v1/lessons/{id}`, after asking); a retired lesson stays on the
 * page with the reason, and can only be deleted.
 *
 * A lesson is a hairline section of the list, read as a sentence first — the situation, what was learned, what to do —
 * with its kind, its cards and how often it was retrieved under it, and its controls at the bottom. Editing turns it
 * into label-left rows; a failure is a toast.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteLesson, editLesson, lessons } from "../../api/calls/improve";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Lesson } from "../../api/types/improve";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Empty } from "../../ui/page/Empty";
import { FieldRow, Rows, TextArea } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { useToast } from "../../ui/page/Toast";
import { lessonMeta, lessonTag } from "./model";

export function Lessons() {
  const queryClient = useQueryClient();
  const toast = useToast();
  const list = useQuery({ queryKey: keys.lessons, queryFn: lessons });
  const [editing, setEditing] = useState<number | null>(null);
  const [asking, setAsking] = useState<number | null>(null);
  const remove = useMutation({
    mutationFn: deleteLesson,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.lessons }),
    onError: (e) =>
      toast({ kind: "error", text: sayError(e, "Could not delete the lesson. Try again.") }),
  });

  // A read that failed is not a Retro that has learned nothing: the empty state would say there is nothing to read.
  if (list.isError) {
    return (
      <ReadFailed
        error={list.error}
        fallback="The lessons could not be read. Try again."
        busy={list.isFetching}
        onRetry={() => void list.refetch()}
      />
    );
  }
  if (!list.data || list.data.length === 0) {
    return (
      <Empty
        title="No lessons yet"
        body="The Retro writes them from failed runs and rejected reviews."
      />
    );
  }
  return (
    <section aria-label="Lessons" className="min-w-0 border-t border-hairline pt-4">
      <div className="flex flex-col divide-y divide-hairline">
        {list.data.map((lesson) =>
          editing === lesson.id ? (
            <LessonEditor
              key={lesson.id}
              lesson={lesson}
              onDone={() => setEditing(null)}
              onSaved={() => queryClient.invalidateQueries({ queryKey: keys.lessons })}
            />
          ) : (
            <LessonCard
              key={lesson.id}
              lesson={lesson}
              asking={asking === lesson.id}
              deleting={remove.isPending}
              onEdit={() => {
                setAsking(null);
                setEditing(lesson.id);
              }}
              onAsk={() => {
                setEditing(null);
                setAsking(lesson.id);
              }}
              onKeep={() => setAsking(null)}
              onDelete={() => {
                setAsking(null);
                remove.mutate(lesson.id);
              }}
            />
          ),
        )}
      </div>
    </section>
  );
}

function LessonCard({
  lesson,
  asking,
  deleting,
  onEdit,
  onAsk,
  onKeep,
  onDelete,
}: {
  lesson: Lesson;
  asking: boolean;
  deleting: boolean;
  onEdit: () => void;
  onAsk: () => void;
  onKeep: () => void;
  onDelete: () => void;
}) {
  const retired = lesson.status === "retired";
  const kind = lessonTag(lesson.kind);
  return (
    <article className="flex min-w-0 flex-col gap-3 py-4 first:pt-0 last:pb-0">
      <h3 className="m-0 min-w-0 break-words text-base font-semibold text-text">
        {lesson.situation}
      </h3>
      <div className="flex flex-wrap gap-2">
        <Tag tone={kind.tone}>{kind.label}</Tag>
        {retired && <Tag tone="plain">Retired</Tag>}
      </div>
      <p className="m-0 min-w-0 break-words text-base leading-normal text-text">{lesson.lesson}</p>
      <p className="m-0 min-w-0 break-words text-base text-text">Do: {lesson.advice}</p>
      <span className="min-w-0 break-words text-sm text-secondary tabular-nums">
        {lessonMeta(lesson)}
      </span>
      <div className="flex flex-wrap gap-2">
        {!retired && (
          <Button
            size="sm"
            variant="secondary"
            aria-label={`Edit the lesson ${lesson.situation}`}
            onClick={onEdit}
          >
            Edit
          </Button>
        )}
        <Button
          size="sm"
          variant="secondary"
          aria-label={`Delete the lesson ${lesson.situation}`}
          onClick={onAsk}
        >
          Delete
        </Button>
      </div>
      {asking && (
        <div className="flex flex-col gap-3 sm:min-h-9 sm:flex-row sm:items-center">
          <p className="m-0 flex min-w-0 flex-1 items-center break-words text-base text-text">
            Delete this lesson?
          </p>
          <div className="flex gap-2">
            <Button size="sm" variant="secondary" onClick={onKeep}>
              Keep
            </Button>
            <Button size="sm" variant="danger" disabled={deleting} onClick={onDelete}>
              Delete lesson
            </Button>
          </div>
        </div>
      )}
    </article>
  );
}

/** A lesson grown into its editor: the situation, what happened and what to do, all needed. */
function LessonEditor({
  lesson,
  onDone,
  onSaved,
}: {
  lesson: Lesson;
  onDone: () => void;
  onSaved: () => Promise<unknown>;
}) {
  const [situation, setSituation] = useState(lesson.situation);
  const [what, setWhat] = useState(lesson.lesson);
  const [advice, setAdvice] = useState(lesson.advice);
  const toast = useToast();
  const save = useMutation({
    mutationFn: () =>
      editLesson(lesson.id, {
        situation: situation.trim(),
        lesson: what.trim(),
        advice: advice.trim(),
      }),
    onSuccess: async () => {
      await onSaved();
      onDone();
    },
    onError: (error) =>
      toast({ kind: "error", text: sayError(error, "Could not save the lesson. Try again.") }),
  });
  const complete = situation.trim() !== "" && what.trim() !== "" && advice.trim() !== "";
  return (
    <form
      className="flex min-w-0 flex-col gap-3 py-4 first:pt-0 last:pb-0"
      onSubmit={(event) => {
        event.preventDefault();
        if (complete) save.mutate();
      }}
    >
      <Rows className="border-t-0">
        <FieldRow label="Situation" wide>
          <Input
            placeholder="The kind of task"
            value={situation}
            onChange={(event) => setSituation(event.target.value)}
          />
        </FieldRow>
        <FieldRow label="What happened" wide>
          <TextArea rows={3} value={what} onChange={(event) => setWhat(event.target.value)} />
        </FieldRow>
        <FieldRow label="What to do" description="The next run is given this sentence." wide>
          <TextArea rows={3} value={advice} onChange={(event) => setAdvice(event.target.value)} />
        </FieldRow>
      </Rows>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" type="submit" disabled={!complete || save.isPending}>
          {save.isPending ? "Saving" : "Save"}
        </Button>
        <Button size="sm" variant="secondary" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </form>
  );
}
