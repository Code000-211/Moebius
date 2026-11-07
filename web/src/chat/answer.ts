/**
 * The person's answer to the agent's questions, as the message that carries it back, and that message read again.
 *
 * The answer is an ordinary message of the person, so the agent reads it and nothing on the server knows it. It is
 * written in one fixed shape, and only that shape is read back to mark the choices of a closed card:
 *
 *     My answers:
 *     1. Database: "SQLite"
 *     2. Extras: "Metrics", "Tracing", Other: "my own words"
 *
 * One line for each question, in the order they were asked: its place, its header (its question when it has none, on
 * one line), a colon, and the answer as a list of JSON strings, the labels of the chosen options as they were written
 * and the person's own words after `Other:`. A label is matched whole, so "Yes" does not mark "Yes, but later", the
 * place and not the header names the question, so two questions of one header are two, and words that happen to hold a
 * label are the person's own words and mark nothing. A message in any other shape marks nothing.
 */
import type { AskedQuestion } from "./conversation";

/** What the person chose for one question: the options by place, and the words of "Other" when it is chosen. */
export interface Choice {
  picked: number[];
  other: boolean;
  words: string;
}

/** What a message that answered a question chose: the options by place, and the person's own words. */
export interface Chosen {
  picked: Set<number>;
  own: string;
}

export const NOTHING: Choice = { picked: [], other: false, words: "" };

/** The first line of a message that answers questions. */
export const ANSWERS_HEADING = "My answers:";

/** The name a question goes by in an answer: its header, or its question, on one line. */
function nameOf(question: AskedQuestion): string {
  return (question.header || question.question).replace(/\s+/g, " ").trim();
}

/** The answer to one question as a list of JSON strings, or null while it has none. */
function answerOf(question: AskedQuestion, choice: Choice): string | null {
  const items = [...choice.picked]
    .sort((a, b) => a - b)
    .flatMap((place) => question.options[place]?.label ?? [])
    .map((label) => JSON.stringify(label));
  const words = choice.words.trim();
  if (choice.other) {
    if (words === "") return null;
    items.push(`Other: ${JSON.stringify(words)}`);
  }
  return items.length > 0 ? items.join(", ") : null;
}

/** The message that answers `questions`, or null while a question has no answer. */
export function answerMessage(questions: AskedQuestion[], choices: Choice[]): string | null {
  const lines: string[] = [];
  for (const [place, question] of questions.entries()) {
    const answer = answerOf(question, choices[place] ?? NOTHING);
    if (answer === null) return null;
    lines.push(`${place + 1}. ${nameOf(question)}: ${answer}`);
  }
  return [ANSWERS_HEADING, ...lines].join("\n");
}

const ITEM = /\s*(Other: )?("(?:[^"\\]|\\.)*")\s*(,|$)/y;

/** The items of an answer, or null when it is not a list of the shape `answerMessage` writes. */
function itemsOf(text: string): Array<{ other: boolean; text: string }> | null {
  const items: Array<{ other: boolean; text: string }> = [];
  ITEM.lastIndex = 0;
  while (ITEM.lastIndex < text.length) {
    const found = ITEM.exec(text);
    if (found === null) return null;
    try {
      const parsed: unknown = JSON.parse(found[2] as string);
      if (typeof parsed !== "string") return null;
      items.push({ other: found[1] !== undefined, text: parsed });
    } catch {
      return null;
    }
    if (found[3] === "") break;
  }
  return items.length > 0 ? items : null;
}

/**
 * What the message `message` chose for each of `questions`: null for a question it does not answer, and for all of them
 * when the message is not in the shape `answerMessage` writes for exactly these questions.
 */
export function answersIn(
  questions: AskedQuestion[],
  message: string | null,
): Array<Chosen | null> {
  const none = questions.map(() => null);
  if (message === null) return none;
  const lines = message.split("\n");
  if (lines.length !== questions.length + 1 || lines[0] !== ANSWERS_HEADING) return none;
  const chosen: Chosen[] = [];
  for (const [place, question] of questions.entries()) {
    const prefix = `${place + 1}. ${nameOf(question)}: `;
    const line = lines[place + 1] as string;
    if (!line.startsWith(prefix)) return none;
    const items = itemsOf(line.slice(prefix.length));
    if (items === null) return none;
    const picked = new Set<number>();
    const own: string[] = [];
    for (const item of items) {
      if (item.other) {
        own.push(item.text);
        continue;
      }
      // A label is one option; a second item of the same label is the next option of that label, if there is one.
      const at = question.options.findIndex(
        (option, each) => option.label === item.text && !picked.has(each),
      );
      if (at < 0) return none;
      picked.add(at);
    }
    chosen.push({ picked, own: own.join(" ") });
  }
  return chosen;
}
