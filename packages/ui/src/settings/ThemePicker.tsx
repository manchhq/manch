import type { JSX } from "react";
export interface ThemePickerProps {
  themes: string[];
  active: string;
  onSelect: (theme: string) => void;
}

export function ThemePicker({ themes, active, onSelect }: ThemePickerProps): JSX.Element {
  return (
    <fieldset className="max-h-64 overflow-y-auto pr-1 grid grid-cols-2 gap-2 sm:grid-cols-3">
      <legend className="mb-2 text-sm font-medium text-base-content">Theme</legend>
      {themes.map((theme) => (
        <label key={theme} className="flex items-center gap-2 rounded-box border border-base-300 px-3 py-2">
          <input
            type="radio"
            name="theme"
            aria-label={theme}
            className="radio radio-primary radio-sm"
            checked={theme === active}
            onChange={() => onSelect(theme)}
          />
          <span className="text-sm capitalize">{theme}</span>
        </label>
      ))}
    </fieldset>
  );
}
