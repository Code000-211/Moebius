/**
 * What a field gives the control inside it: the classes every control shares, and the ids of the two lines that
 * describe it — the label above and the hint or error under.
 *
 * A control that reads them names itself with the label alone and points at its explanation with `aria-describedby`.
 * That matters for a `select`, whose own text is the list of choices: wrapped in a `<label>` it would be read out as
 * "Kind, command, url" rather than "Kind".
 */
import { createContext, useContext } from "react";

/**
 * The classes every form control shares, so a text field, a select and a multi-line field are the same control drawn
 * three ways: on the field colour — the lightest surface in the card, so the thing to type into is the brightest
 * thing in it — at the control radius, at least 40px tall, in body text, with its edge drawn inside the box rather
 * than as a border. It lives here rather than in `page/Field.tsx` because `components/Input.tsx` needs it too and a
 * primitive must not reach up into the page pieces.
 */
export const CONTROL =
  "min-h-10 w-full rounded-control bg-field field-edge px-3 py-2 text-base font-normal text-text placeholder:text-muted disabled:cursor-not-allowed disabled:opacity-45";

/** The two lines of the field a control sits in. */
interface FieldIds {
  /** The label above the control, which is its name. */
  label?: string;
  /** The hint or the error under the control, which explains it. */
  note?: string;
}

const ids = createContext<FieldIds>({});

/**
 * The ids to point `aria-labelledby` and `aria-describedby` at, when this control sits in a field.
 *
 * A control that names itself wins: `aria-labelledby` beats `aria-label` for a screen reader, so a field may only name
 * a control that has no name of its own. One card often holds a field whose label is the short word and a control that
 * has to say more than that — the key of the third upstream is not "Key" — so the field stays quiet there. The hint
 * still describes the control either way.
 */
export function useFieldIds(explicit?: {
  /** The `aria-label` the caller gave the control, if it named itself. */
  label?: string;
  labelledBy?: string;
  describedBy?: string;
}) {
  const fromField = useContext(ids);
  const namesItself = explicit?.label !== undefined || explicit?.labelledBy !== undefined;
  return {
    labelledBy: namesItself ? explicit?.labelledBy : fromField.label,
    describedBy: explicit?.describedBy ?? fromField.note,
  };
}

/** Tell the controls below a field the ids of the lines that describe it. */
export const FieldIds = ids.Provider;
