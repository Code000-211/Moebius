/**
 * The lessons box of the Retro page: what the Retro learned from earlier cards (`GET /v1/lessons`), each with its
 * kind, the cards it comes from and how often a run was given it. A person edits a lesson in place
 * (`PUT /v1/lessons/{id}`) and deletes it (`DELETE /v1/lessons/{id}`, after asking); a retired lesson stays on the
 * page with the reason, and can only be deleted.
 */
import { PencilSimpleIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteLesson, editLesson, lessons } from "../../api/calls/improve";
import { keys } from "../../api/keys";
import type { Lesson } from "../../api/types/improve";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Body, Box, Field, Notice, sayError, TextArea } from "../kit/Kit";
import { lessonMeta, lessonTag } from "./model";

export function Lessons() {
  const queryClient = useQueryClient();
  const list = useQuery({ queryKey: keys.lessons, queryFn: lessons });
  const [editing, setEditing] = useState<number | null>(null);
  const [asking, setAsking] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const remove = useMutation({
    mutationFn: deleteLesson,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.lessons }),
    onError: (e) => setError(sayError(e, "Could not delete the lesson.")),
  });

  if (!list.data || list.data.length === 0) {
    return (
      <Box title="Lessons">
        <Body>
          None yet. The Retro writes them from failed runs, rejected reviews and cards that merged
          at once.
        </Body>
      </Box>
    );
  }
  return (
    <Box title="Lessons">
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
              setError(null);
              setEditing(lesson.id);
            }}
            onAsk={() => {
              setEditing(null);
              setError(null);
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
      {error && editing === null && <Notice tone="error">{error}</Notice>}
    </Box>
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
    <article className="flex flex-col gap-half rounded-lg border bg-lane p-base">
      <div className="flex flex-wrap items-center gap-half">
        <h3 className="break-words text-sm font-semibold text-high">{lesson.situation}</h3>
        {!retired && (
          <button
            type="button"
            aria-label={`Edit the lesson ${lesson.situation}`}
            title="Edit the lesson"
            onClick={onEdit}
            className="rounded-sm p-half text-low hover:bg-card hover:text-high"
          >
            <PencilSimpleIcon className="size-icon-sm" />
          </button>
        )}
        <span className="ml-auto flex flex-wrap items-center gap-half">
          <Tag tone={kind.tone}>{kind.label}</Tag>
          {retired && <Tag>Retired</Tag>}
          {asking ? (
            <span className="flex items-center gap-base text-sm text-normal">
              Delete this lesson?
              <Button size="xs" variant="outline" onClick={onKeep}>
                Keep
              </Button>
              <Button size="xs" variant="destructive" disabled={deleting} onClick={onDelete}>
                Delete lesson
              </Button>
            </span>
          ) : (
            <Button
              size="xs"
              variant="outline"
              aria-label={`Delete the lesson ${lesson.situation}`}
              onClick={onAsk}
            >
              Delete
            </Button>
          )}
        </span>
      </div>
      <p className="break-words text-sm text-normal">{lesson.lesson}</p>
      <p className="break-words text-sm text-high">Do: {lesson.advice}</p>
      <span className="break-words font-mono text-xs text-low">{lessonMeta(lesson)}</span>
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
  });
  const complete = situation.trim() !== "" && what.trim() !== "" && advice.trim() !== "";
  return (
    <div className="flex flex-col gap-base rounded-lg border border-accent bg-card p-base">
      <Field label="Situation">
        {(id) => (
          <Input
            id={id}
            placeholder="The kind of task"
            value={situation}
            onChange={(event) => setSituation(event.target.value)}
          />
        )}
      </Field>
      <Field label="What happened">
        {(id) => (
          <TextArea
            id={id}
            rows={3}
            value={what}
            onChange={(event) => setWhat(event.target.value)}
          />
        )}
      </Field>
      <Field label="What to do">
        {(id) => (
          <TextArea
            id={id}
            rows={3}
            value={advice}
            onChange={(event) => setAdvice(event.target.value)}
          />
        )}
      </Field>
      {save.error && <Notice tone="error">{sayError(save.error, "Could not save.")}</Notice>}
      <div className="flex gap-base">
        <Button size="sm" disabled={!complete || save.isPending} onClick={() => save.mutate()}>
          Save
        </Button>
        <Button size="sm" variant="outline" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
