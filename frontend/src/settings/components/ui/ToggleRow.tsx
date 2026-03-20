interface ToggleRowProps {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  className?: string;
}

export function ToggleRow({ label, checked, onChange, className = "" }: ToggleRowProps) {
  return (
    <label className={`toggle-row ${className}`}>
      <span>{label}</span>
      <input
        type="checkbox"
        className="toggle-switch"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}
