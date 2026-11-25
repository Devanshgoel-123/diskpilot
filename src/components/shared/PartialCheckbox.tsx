/**
 * A checkbox for a group: checked when every item is selected, a dash
 * (indeterminate) when only some are.
 */
export function PartialCheckbox({
  checked,
  partial = false,
  onChange,
  label,
  title,
}: {
  checked: boolean;
  partial?: boolean;
  onChange: () => void;
  label: string;
  title?: string;
}) {
  return (
    <input
      type="checkbox"
      checked={checked}
      ref={(el) => {
        if (el) el.indeterminate = partial && !checked;
      }}
      onChange={onChange}
      aria-label={label}
      title={title}
      className="size-3.5 shrink-0 accent-[var(--accent)]"
    />
  );
}
