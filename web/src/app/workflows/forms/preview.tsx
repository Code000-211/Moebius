/** The parameters as the file will write them, shown under the fields that change them. */
import { asJson } from "../../../ui/page/JsonValue";

export function ParamsPreview({ params }: { params: Record<string, unknown> }) {
  return (
    <section
      aria-label="Saved parameters"
      className="flex flex-col gap-2 border-t border-hairline pt-3"
    >
      <h3 className="m-0 text-xs font-semibold uppercase tracking-wide text-secondary">
        Saved JSON
      </h3>
      <pre
        data-testid="params-preview"
        className="m-0 overflow-x-auto whitespace-pre-wrap break-words rounded-control bg-code px-3 py-2 font-mono text-xs leading-5 text-text"
      >
        {asJson(params)}
      </pre>
    </section>
  );
}
