# `manual`

The trigger a person starts. Its run does not wait for a card or a schedule: it starts when a person asks for it,
through the app, `POST /v1/workflows/{name}/runs` or `moebius workflow run`, and it starts with the inputs the trigger
names. A run started this way is recorded with `manual` as the kind of the trigger that started it.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `inputs` | array of tables | What a person gives the run, in the order a form shows them. Optional; a trigger with no inputs starts a run that carries nothing but what its defaults give. |

Each input is a table with:

| Key | Type | Meaning |
| --- | --- | --- |
| `name` | string | The name the run's inputs give it by, and the name a refusal names it by. |
| `type` | string | What a value of the input must be: `text`, `number`, `boolean`, `choice` or `repository`. |
| `required` | boolean | Whether a run must give the input a value. Optional; defaults to `false`. |
| `default` | any JSON value | The value the input takes when a run gives it none. Optional. |
| `choices` | list of strings | The values a `choice` input may be. Only for a `choice`, and not empty. |

The types:

| Type | A value of it is |
| --- | --- |
| `text` | A JSON string. |
| `number` | A JSON number. |
| `boolean` | `true` or `false`. |
| `choice` | A string out of the input's `choices`. |
| `repository` | The name of a repository the server knows. A trigger with such an input takes the repository its run belongs to from that input, and then takes no `repository` of its own. |

A value that is given wins over the default; a default stands in for a value that was not given; an input that is
neither given nor defaulted is left out of the value the run starts with, unless it is required.

```toml
[[trigger]]
kind = "manual"

[[trigger.inputs]]
name = "version"
type = "text"
required = true

[[trigger.inputs]]
name = "count"
type = "number"
default = 3

[[trigger.inputs]]
name = "dry_run"
type = "boolean"
default = true

[[trigger.inputs]]
name = "environment"
type = "choice"
choices = ["staging", "prod"]
default = "prod"

[[trigger.inputs]]
name = "checkout"
type = "repository"
```

## Ports

A trigger has no input ports: it is where the value comes from. One output, `out`, carries the value the run starts
with — the inputs a person gave as one object, with the defaults standing in — and the edges leaving the trigger name
that port:

```toml
[[edges]]
from = "trigger"
to = "build"
port = "out"
```

## Reading the inputs

`GET /v1/workflows/{name}/run-inputs` answers the inputs in the order the file names them, so a form builds itself
from them; see the [desktop API](../desktop-api.md#get-v1workflowsnamerun-inputs). Each input answers with `name`,
`type`, `required`, and `default` and `choices` only when it has them.

## Starting a run

`POST /v1/workflows/{name}/runs` takes the inputs by their names and the repository the run belongs to, and answers the
run as it was recorded. `moebius workflow run <name>` is the same door: `--input name=value` for each input, the text
taken as the type the trigger declares, or `--inputs <file|->` for a JSON object of them all. See the
[command line](../cli.md) and the [desktop API](../desktop-api.md#post-v1workflowsnameruns).

## Validation

Reading a workflow file refuses a `manual` trigger, naming the input each message is about:

- a parameter that is not `inputs`: `trigger: unknown trigger parameters ["count"]`;
- `inputs` is not a list of objects: `trigger: input 1: an input is an object of its fields`;
- an input has no `name`, or one that is not a non-empty string: `trigger: input 0: "name" is a non-empty string`;
- an input has a key the table above does not give: `trigger: input "count": unknown key "type2"`;
- `type` is missing: `trigger: input "count": "type" is one of the input's types`, and a `type` that is not one of
  the five: `trigger: input "count": "type" is one of "text", "number", "boolean", "choice", "repository"`;
- `required` is neither `true` nor `false`: `trigger: input "count": "required" is true or false`;
- `choices` is not a non-empty list of strings, lists one twice, or sits on an input that is not a `choice`: `trigger: input "environment": "choices" lists "prod" twice`;
- two inputs have the same name: `trigger: input "count": duplicate input name`;
- `default` is not a value of the input's own type, or not one of its choices: `trigger: input "count": "default": must be a JSON number`.

A name that is not a safe file name is refused as it always is, and a trigger of another kind is read by the node
registry as before.

## Starting a run that is not right

Starting a run checks the given inputs the same way, and the first input that is not right stops it, named:

- a value that is not of its input's type: `input "count": must be a JSON number`;
- a `choice` that is not one of its choices: `input "environment": must be one of the input's choices`;
- a required input with neither a value nor a default: `input "version": is required`;
- a name the trigger does not give: `input "other": the trigger has no input named "other"`.

A value that is given is checked before an input that is missing, so a person who gave a wrong value hears about the
input they gave. The workflow's own checks come first and are not about one input:

- the workflow's trigger is not `manual`: `the workflow's trigger is "card", not manual` (a 409, as is a workflow with
  no trigger);
- the trigger names a `repository` input and the request also gives `repository`: `the trigger's input "checkout" names the run's repository, so "repository" is not taken`;
- the trigger names a `repository` input and no value was given for it: `the trigger's input "checkout" names the run's repository, but no value was given for it`;
- the trigger names no `repository` input and the request gives no `repository`: `the trigger has no repository input, so "repository" names the run's repository`;
- no repository of that name is known: `input "checkout": no repository named "radiance"`, or `no repository named "radiance"` when `repository` named it.
