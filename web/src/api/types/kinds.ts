/** Types of the calls about kinds of issues and their fields; they follow docs/reference/desktop-api.md. */

/** The type of a field of a kind. */
export type FieldType = "text" | "number" | "choice" | "date" | "person";

export const FIELD_TYPES: Array<[FieldType, string]> = [
  ["text", "Text"],
  ["number", "Number"],
  ["choice", "Choice"],
  ["date", "Date"],
  ["person", "Person"],
];

/** A field of a kind (`GET /v1/kinds`). */
export interface KindField {
  id: number;
  name: string;
  type: FieldType;
  /** The values of a choice; empty for any other type. */
  options: string[];
}

/** A kind of issue with its fields and the ids of the projects that offer it. */
export interface Kind {
  id: number;
  name: string;
  fields: KindField[];
  projects: number[];
}

/** A field as it is written when a kind is made or replaced. */
export interface FieldDef {
  name: string;
  type: FieldType;
  options: string[];
}

/** The body of `POST /v1/kinds` and `PUT /v1/kinds/{id}`. */
export interface KindBody {
  name: string;
  fields: FieldDef[];
}

/** The value a card has for a field: a number for a number field, text for the others. */
export type FieldValue = string | number;
