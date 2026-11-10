/**
 * The questions the agent put to the person with its question tool, drawn where it asked them: each with a few options
 * to choose from (one, or several when the question allows it) and an "Other" that takes the person's own words. The
 * answers go back as the person's next message, so the card needs no channel of its own. It is a form while the agent
 * waits on it and a plain record once the conversation went on, because the options of an answered question are no
 * longer something to press.
 */
import { CheckIcon } from "@phosphor-icons/react";
import { useId, useState } from "react";
import { answerMessage, answersIn, type Choice, type Chosen, NOTHING } from "../../chat/answer";
import type { AskedQuestion } from "../../chat/conversation";
import { Button } from "../components/Button";
import { Input } from "../components/Input";
import { cn } from "../lib/cn";
import { Tag } from "./Tag";

/**
 * A key for each of `texts`: the text, and for the second of the same text and the ones after it, its number. Two
 * questions or two options that read alike are two things with a state of their own, and neither takes the other's.
 */
function keysOf(texts: string[]): string[] {
  const seen = new Map<string, number>();
  return texts.map((text) => {
    const count = (seen.get(text) ?? 0) + 1;
    seen.set(text, count);
    return count === 1 ? text : `${text}#${count}`;
  });
}

/** The longest header and option label drawn whole; the tool asks for no more, and a longer one is cut here. */
const HEADER_MAX = 16;
const LABEL_MAX = 60;

/** `text` cut to `max` characters with the whole of it as the tooltip, or as it is when it fits. */
function Clipped({ text, max }: { text: string; max: number }) {
  const letters = Array.from(text);
  if (letters.length <= max) return <>{text}</>;
  return <span title={text}>{`${letters.slice(0, max - 1).join("")}…`}</span>;
}

/** A question the conversation went past: its options, with the person's choice marked. */
function ClosedQuestion({ question, said }: { question: AskedQuestion; said: Chosen | null }) {
  const { picked, own } = said ?? { picked: new Set<number>(), own: "" };
  const optionKeys = keysOf(question.options.map((option) => option.label));
  const row = (chosen: boolean) =>
    cn("flex items-start gap-2 text-base", chosen ? "font-semibold text-text" : "text-secondary");
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <p className="m-0 flex flex-col items-start gap-2 text-base font-semibold text-text">
        {question.header && (
          <Tag>
            <Clipped text={question.header} max={HEADER_MAX} />
          </Tag>
        )}
        <span className="break-words">{question.question}</span>
      </p>
      <ul className="m-0 flex list-none flex-col gap-1 p-0">
        {question.options.map((option, place) => (
          <li key={optionKeys[place]} className={row(picked.has(place))}>
            <span className="mt-1 size-4 shrink-0">
              {picked.has(place) && (
                <CheckIcon aria-hidden="true" className="size-4" weight="bold" />
              )}
            </span>
            <span className="break-words">
              <Clipped text={option.label} max={LABEL_MAX} />
              {picked.has(place) && <span className="sr-only"> (chosen)</span>}
            </span>
            {option.recommended && <Tag tone="complete">Recommended</Tag>}
          </li>
        ))}
        {own !== "" && (
          <li className={row(true)}>
            <CheckIcon aria-hidden="true" className="mt-1 size-4 shrink-0" weight="bold" />
            <span className="break-words">
              <span className="sr-only">Chosen, </span>Other: {own}
            </span>
          </li>
        )}
      </ul>
    </div>
  );
}

function QuestionField({
  question,
  choice,
  onChange,
}: {
  question: AskedQuestion;
  choice: Choice;
  onChange: (next: Choice) => void;
}) {
  const name = useId();
  const optionKeys = keysOf(question.options.map((option) => option.label));
  const row =
    "flex min-h-10 cursor-pointer items-start gap-3 rounded-control px-3 py-2 text-base text-text hover:bg-hover";
  const pick = (place: number, on: boolean) => {
    if (question.multiple) {
      const picked = on
        ? [...choice.picked, place]
        : choice.picked.filter((each) => each !== place);
      onChange({ ...choice, picked });
    } else {
      onChange({ picked: [place], other: false, words: choice.words });
    }
  };
  const kind = question.multiple ? "checkbox" : "radio";
  return (
    <fieldset className="m-0 flex min-w-0 flex-col gap-1 border-0 p-0">
      <legend className="mb-2 flex flex-col items-start gap-2 p-0 text-base font-semibold text-text">
        {question.header && (
          <Tag>
            <Clipped text={question.header} max={HEADER_MAX} />
          </Tag>
        )}
        <span className="break-words">{question.question}</span>
      </legend>
      {question.options.map((option, place) => (
        <label key={optionKeys[place]} className={row}>
          <input
            type={kind}
            name={name}
            className="mt-1 size-4 shrink-0 accent-current"
            checked={choice.picked.includes(place)}
            onChange={(event) => pick(place, event.target.checked)}
          />
          <span className="flex min-w-0 flex-col">
            <span className="flex flex-wrap items-center gap-2 break-words">
              <Clipped text={option.label} max={LABEL_MAX} />
              {option.recommended && <Tag tone="complete">Recommended</Tag>}
            </span>
            {option.description && (
              <span className="break-words text-sm text-secondary">{option.description}</span>
            )}
          </span>
        </label>
      ))}
      <label className={row}>
        <input
          type={kind}
          name={name}
          className="mt-1 size-4 shrink-0 accent-current"
          checked={choice.other}
          onChange={(event) =>
            onChange(
              question.multiple
                ? { ...choice, other: event.target.checked }
                : { picked: [], other: event.target.checked, words: choice.words },
            )
          }
        />
        <span>Other</span>
      </label>
      {choice.other && (
        <Input
          aria-label={`Your answer to: ${question.question}`}
          placeholder="Write your own answer"
          value={choice.words}
          autoFocus
          onChange={(event) => onChange({ ...choice, words: event.target.value })}
          className="ml-10 w-auto"
        />
      )}
    </fieldset>
  );
}

interface ChatQuestionProps {
  questions: AskedQuestion[];
  /** The person's message that answered the questions, for a card the conversation went past; null when none did. */
  answer?: string | null;
  /** Sends the answers as the person's message; leave it out for a question the conversation went past. */
  onAnswer?: (message: string) => void;
  /** Whether sending is on its way, so the answer is not sent twice. */
  sending?: boolean;
}

export function ChatQuestion({ questions, answer, onAnswer, sending = false }: ChatQuestionProps) {
  const [choices, setChoices] = useState<Choice[]>([]);
  const message = answerMessage(questions, choices);
  const questionKeys = keysOf(questions.map((question) => question.question));
  if (!onAnswer) {
    const said = answersIn(questions, answer ?? null);
    return (
      <div
        data-testid="question-asked"
        className="flex min-w-0 flex-col gap-4 rounded-card bg-sunken p-4"
      >
        {questions.map((question, place) => (
          <ClosedQuestion
            key={questionKeys[place]}
            question={question}
            said={said[place] ?? null}
          />
        ))}
      </div>
    );
  }
  return (
    <form
      data-testid="question"
      aria-label="The agent asks"
      className={cn("flex min-w-0 flex-col gap-4 rounded-card bg-sunken p-4")}
      onSubmit={(event) => {
        event.preventDefault();
        if (message !== null && !sending) onAnswer(message);
      }}
    >
      {questions.map((question, place) => (
        <QuestionField
          key={questionKeys[place]}
          question={question}
          choice={choices[place] ?? NOTHING}
          onChange={(next) =>
            setChoices((held) => {
              const copy = questions.map((_, at) => held[at] ?? NOTHING);
              copy[place] = next;
              return copy;
            })
          }
        />
      ))}
      <Button type="submit" size="sm" disabled={message === null || sending}>
        Send answer
      </Button>
    </form>
  );
}
