interface SliderFieldProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  onChange: (value: number) => void;
  className?: string;
}

export function SliderField({
  label, value, min, max, step = 1, unit = "", onChange, className = "",
}: SliderFieldProps) {
  return (
    <div className={className}>
      <label className="field-label">
        {label}: {value}{unit}
      </label>
      <input
        type="range"
        className="slider"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}
