/** The form of an `http` node: the URL and method it calls, the secret it names and how long it waits. */

import { Input } from "../../../ui/components/Input";
import { Field, Select } from "../../../ui/page/Field";
import type { NodeFormProps } from "../nodeKinds";
import { ParamsPreview } from "./preview";
import { SecretField } from "./secrets";

const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

export function HttpForm({ node, onParams }: NodeFormProps) {
  const params = (node.params ?? {}) as Record<string, unknown>;
  const url = typeof params.url === "string" ? params.url : "";
  const method = typeof params.method === "string" && params.method !== "" ? params.method : "GET";
  const secret = typeof params.secret === "string" ? params.secret : "";
  const timeout = typeof params.timeout_seconds === "number" ? String(params.timeout_seconds) : "";
  const missing = url.trim() === "";

  function give(name: string, value: unknown) {
    const next = { ...params };
    if (value === undefined || value === "") {
      delete next[name];
    } else {
      next[name] = value;
    }
    onParams(next);
  }

  return (
    <div className="flex min-w-0 flex-col gap-4" data-testid="http-form">
      <Field
        label="URL"
        hint="Where the node calls. Expressions such as {{ trigger.tag }} are filled before it runs."
        error={missing ? "An http node needs its URL." : undefined}
      >
        <Input
          aria-label="URL"
          mono
          value={url}
          placeholder="https://example.com/hook"
          onChange={(event) => give("url", event.target.value)}
        />
      </Field>
      <Field label="Method">
        <Select
          aria-label="Method"
          value={method}
          onChange={(value) => give("method", value)}
          options={METHODS.map((each): [string, string] => [each, each])}
        />
      </Field>
      <SecretField
        label="Secret"
        value={secret}
        onChange={(value) => give("secret", value)}
        hint="The name of a secret the server holds; its value is added to the call and never written here."
      />
      <Field
        label="Timeout"
        hint="How long the call may take, in seconds. Left alone, the kind's own limit."
      >
        <Input
          type="number"
          inputMode="numeric"
          min={1}
          aria-label="Timeout"
          value={timeout}
          onChange={(event) => {
            const value = event.target.value;
            give("timeout_seconds", value.trim() === "" ? undefined : Number(value));
          }}
        />
      </Field>
      <ParamsPreview params={params} />
    </div>
  );
}
