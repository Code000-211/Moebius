/**
 * The "Stay signed in" choice of the sign-in page and the connect dialog, on by default. On, the sign-in outlives the
 * browser or the app being closed; off, it is kept for this session only.
 */
export function StayCheckbox({
  checked,
  onChange,
  name,
}: {
  /** Left out, the box is not controlled: it starts on and is read from the form as `name`. */
  checked?: boolean;
  onChange?: (checked: boolean) => void;
  name?: string;
}) {
  return (
    <label className="flex w-fit items-center gap-2 text-sm text-secondary">
      <input
        type="checkbox"
        name={name}
        className="size-4 accent-accent"
        {...(onChange
          ? { checked: checked ?? true, onChange: (event) => onChange(event.target.checked) }
          : { defaultChecked: true })}
      />
      Stay signed in
    </label>
  );
}
