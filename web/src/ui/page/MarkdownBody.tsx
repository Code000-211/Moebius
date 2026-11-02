/**
 * The text of a Markdown file with the choice of how to read it: the text as written, or the same text drawn as
 * headings, lists and code. A skill is a document a person reads as often as they edit it, so both are offered, and it
 * opens as the text: that is what they came to look at, and the drawing is one tab away. The raw side is the text area
 * the page had on its own, so what is typed and how it is saved do not change; the drawn side is the Markdown the chat
 * uses. The choice of how to read belongs to whoever shows the file, not to this component, so moving on to another
 * file keeps it.
 */
import { useId } from "react";
import { Markdown } from "../chat/Markdown";
import { TextArea } from "./Field";
import { TabPanel, Tabs } from "./Tabs";

/** How a Markdown file is read: as the text, or drawn. */
export type MarkdownView = "raw" | "rendered";

const VIEWS: Array<{ value: MarkdownView; label: string }> = [
  { value: "raw", label: "Raw" },
  { value: "rendered", label: "Rendered" },
];

export function MarkdownBody({
  label,
  text,
  onChange,
  readOnly = false,
  placeholder,
  rows = 18,
  view,
  onViewChange,
}: {
  /** What the text is: the name of the text area and of the row of tabs. */
  label: string;
  text: string;
  /** Absent for text that cannot be changed. */
  onChange?: (text: string) => void;
  readOnly?: boolean;
  placeholder?: string;
  rows?: number;
  /** How it is read now, held by whoever shows the file, so the next one is read the same way. */
  view: MarkdownView;
  onViewChange: (view: MarkdownView) => void;
}) {
  const idBase = useId();
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <Tabs
        idBase={idBase}
        label={`View of ${label}`}
        tabs={VIEWS}
        value={view}
        onChange={onViewChange}
      />
      {view === "raw" ? (
        <TabPanel idBase={idBase} value="raw">
          <TextArea
            rows={rows}
            aria-label={label}
            value={text}
            readOnly={readOnly}
            placeholder={placeholder}
            onChange={onChange && ((event) => onChange(event.target.value))}
          />
        </TabPanel>
      ) : (
        <TabPanel idBase={idBase} value="rendered">
          {text.trim() === "" ? (
            <p className="m-0 text-base text-secondary">Nothing written yet.</p>
          ) : (
            <Markdown content={text} className="max-w-prose" />
          )}
        </TabPanel>
      )}
    </div>
  );
}
