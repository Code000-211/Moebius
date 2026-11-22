/**
 * The parameters as the file will write them, under the fields that change them. It is folded away until someone asks
 * for it: the fields above say the same in a form to read, and JSON writes a line break as `\n`.
 */
import { asJson } from "../../../ui/page/JsonValue";

export function ParamsPreview({ params }: { params: Record<string, unknown> }) {
  return (
    <details aria-label="Saved parameters" className="border-t border-hairline pt-3">
      <summary className="cursor-pointer text-xs font-semibold uppercase tracking-wide text-secondary">
        Saved JSON
      </summary>
      <pre
        data-testid="params-preview"
        className="m-0 mt-2 overflow-x-auto whitespace-pre-wrap break-words rounded-panel bg-code-bg px-3 py-2 font-mono text-xs leading-5 text-code-text"
      >
        {asJson(params)}
      </pre>
    </details>
  );
}
