/**
 * The category of a page: one of the folders of the group the page is in (the organisation's pages, or one project's),
 * "None" for the top level, or "+ New category", which asks for its name. A category is a folder, so a slash in a new
 * category's name makes a subcategory. The new page's form and the Move dialog both use it, so a page is sorted the same
 * way wherever it is put.
 */
import { useState } from "react";
import { Input } from "../../ui/components/Input";
import { Field, Select } from "../../ui/page/Field";

const NONE = "\u0000none";
const NEW = "\u0000new";

export function CategoryPicker({
  categories,
  value,
  onChange,
  problem,
}: {
  /** The categories to choose from, as `categoriesOf` lists them. */
  categories: readonly string[];
  /** The category now; empty for none. */
  value: string;
  onChange: (category: string) => void;
  /** Why the category typed is no folder's name, when it is not. */
  problem?: string | null;
}) {
  // A category that is not among the folders (a new one, or an address that names a folder with no page yet) is typed.
  const [creating, setCreating] = useState(value !== "" && !categories.includes(value));
  const choice = creating ? NEW : value === "" ? NONE : value;
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <Field label="Category">
        <Select
          aria-label="Category"
          options={[
            [NONE, "None"],
            ...categories.map((each): [string, string] => [each, each]),
            [NEW, "+ New category"],
          ]}
          value={choice}
          onChange={(next) => {
            if (next === NEW) {
              setCreating(true);
              onChange("");
              return;
            }
            setCreating(false);
            onChange(next === NONE ? "" : next);
          }}
        />
      </Field>
      {creating && (
        <Field
          label="Name of the new category"
          hint="A slash makes a subcategory: Runbooks/Old."
          error={value.trim() !== "" && problem ? problem : undefined}
        >
          <Input
            value={value}
            autoComplete="off"
            placeholder="Runbooks"
            onChange={(event) => onChange(event.target.value)}
          />
        </Field>
      )}
    </div>
  );
}
