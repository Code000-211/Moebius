/**
 * The logic of the Skills page, free of React: what a save of a skill and of one of its files sends, or what is wrong
 * with the fields (`screens/skills/` of the former desktop app), and how a size reads.
 */

const NAME = /^[A-Za-z0-9_-]+$/;

export interface SkillRequest {
  name: string;
  description: string;
  text: string;
}

/** The skill's name, description and text as a `PUT` sends them, or what is wrong with the fields. */
export function skillRequest(fields: {
  name: string;
  description: string;
  text: string;
}): SkillRequest | string {
  const name = fields.name.trim();
  if (!NAME.test(name)) return "A name is made of letters, digits, - and _.";
  const description = fields.description.trim();
  if (description === "") return "Enter the description.";
  // The file is not given a second final line break.
  const text = fields.text.endsWith("\n") ? fields.text.slice(0, -1) : fields.text;
  return { name, description, text };
}

/** The path and text a save of one of the skill's other files sends, or what is wrong with them. */
export function fileRequest(
  path: string,
  text: string | null,
): { path: string; text: string } | string {
  const trimmed = path.trim().replace(/^\/+|\/+$/g, "");
  if (trimmed === "" || trimmed.split("/").some((part) => part === "" || part === "..")) {
    return "Enter the file's path inside the skill, such as scripts/fill.py.";
  }
  if (trimmed === "SKILL.md") return "SKILL.md is saved with the skill.";
  if (text === null) return "This file is not text and cannot be edited here.";
  return { path: trimmed, text };
}

/** A size in bytes as a person reads it. */
export function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
