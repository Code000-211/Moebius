/**
 * The questions the agent put to the person with its question tool, drawn where it asked them: each with a few options
 * to choose from (one, or several when the question allows it) and an "Other" that takes the person's own words. The
 * answers go back as the person's next message, so the card needs no channel of its own. It is a form while the agent
 * waits on it and a plain record once the conversation went on, because the options of an answered question are no
 * longer something to press.
 */
import { CheckIcon } from "@phosphor-icons/react";
import { useId, useState } from "react";
import type { AskedQuestion } from "../../chat/conversation";
import { Button } from "../components/Button";
import { Input } from "../components/Input";
import { cn } from "../lib/cn";
import { Tag } from "./Tag";

/** What the person chose for one question: the options by place, and the words of "Other" when it is chosen. */
interface Choice {
  picked: number[];
  other: boolean;
  words: string;
}

const NOTHING: Choice = { picked: [], other: false, words: "" };

/** The answer to one question in words, or null while it has none. */
export function answerOf(question: AskedQuestion, choice: Choice): string | null {
  const labels = choice.picked.flatMap((place) => question.options[place]?.label ?? []);
  const words = choice.words.trim();
  if (choice.other) {
    if (words === "") return null;
    labels.push(words);
  }
  return labels.length > 0 ? labels.join(", ") : null;
}

/**
 * The message that answers `questions`: the answer alone for one question, else a line for each, named by its header
 * (or by the question when it has none). Null while a question has no answer.
 */
export function answerMessage(questions: AskedQuestion[], choices: Choice[]): string | null {
  const answers = questions.map((question, place) => answerOf(question, choices[place] ?? NOTHING));
  if (answers.some((answer) => answer === null)) return null;
  const [only] = answers;
  if (questions.length === 1 && only) return only;
  return questions.map((q, place) => `${q.header || q.question}: ${answers[place]}`).join("\n");
}

/** What the person said to each question, from the message that answered them all (see `answerMessage`). */
export function answersIn(
  questions: AskedQuestion[],
  message: string | null,
): Array<string | null> {
  if (message === null) return questions.map(() => null);
  if (questions.length === 1) return [message];
  const lines = message.split("\n");
  return questions.map((question) => {
    const prefix = `${question.header || question.question}: `;
    const line = lines.find((each) => each.startsWith(prefix));
    return line === undefined ? null : line.slice(prefix.length);
  });
}

/** The options of `question` that `said` names, and the words left over, which are the person's own answer. */
export function chosenIn(
  question: AskedQuestion,
  said: string,
): { picked: Set<number>; own: string } {
  const picked = new Set<number>();
  let rest = said;
  const longest = question.options
    .map((option, place) => ({ label: option.label, place }))
    .sort((a, b) => b.label.length - a.label.length);
  for (const { label, place } of longest) {
    const at = rest.indexOf(label);
    if (at < 0) continue;
    picked.add(place);
    rest = `${rest.slice(0, at)}\u0000${rest.slice(at + label.length)}`;
  }
  const own = rest
    .split("\u0000")
    .map((piece) => piece.replace(/^[\s,;]+|[\s,;]+$/g, ""))
    .filter((piece) => piece !== "")
    .join(" ");
  return { picked, own };
}

/** A question the conversation went past: its options, with the person's choice marked. */
function ClosedQuestion({ question, said }: { question: AskedQuestion; said: string | null }) {
  const { picked, own } =
    said === null ? { picked: new Set<number>(), own: "" } : chosenIn(question, said);
  const row = (chosen: boolean) =>
    cn("flex items-start gap-2 text-base", chosen ? "font-semibold text-text" : "text-secondary");
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <p className="m-0 flex flex-col items-start gap-2 text-base font-semibold text-text">
        {question.header && <Tag>{question.header}</Tag>}
        <span className="break-words">{question.question}</span>
      </p>
      <ul className="m-0 flex list-none flex-col gap-1 p-0">
        {question.options.map((option, place) => (
          <li key={option.label} className={row(picked.has(place))}>
            <span className="mt-1 size-4 shrink-0">
              {picked.has(place) && (
                <CheckIcon aria-hidden="true" className="size-4" weight="bold" />
              )}
            </span>
            <span className="break-words">
              {option.label}
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
        {question.header && <Tag>{question.header}</Tag>}
        <span className="break-words">{question.question}</span>
      </legend>
      {question.options.map((option, place) => (
        <label key={option.label} className={row}>
          <input
            type={kind}
            name={name}
            className="mt-1 size-4 shrink-0 accent-current"
            checked={choice.picked.includes(place)}
            onChange={(event) => pick(place, event.target.checked)}
          />
          <span className="flex min-w-0 flex-col">
            <span className="flex flex-wrap items-center gap-2 break-words">
              {option.label}
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
  if (!onAnswer) {
    const said = answersIn(questions, answer ?? null);
    return (
      <div
        data-testid="question-asked"
        className="flex max-w-prose min-w-0 flex-col gap-4 rounded-card bg-sunken p-4"
      >
        {questions.map((question, place) => (
          <ClosedQuestion key={question.question} question={question} said={said[place] ?? null} />
        ))}
      </div>
    );
  }
  return (
    <form
      data-testid="question"
      aria-label="The agent asks"
      className={cn("flex max-w-prose min-w-0 flex-col gap-4 rounded-card bg-sunken p-4")}
      onSubmit={(event) => {
        event.preventDefault();
        if (message !== null && !sending) onAnswer(message);
      }}
    >
      {questions.map((question, place) => (
        <QuestionField
          key={question.question}
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
