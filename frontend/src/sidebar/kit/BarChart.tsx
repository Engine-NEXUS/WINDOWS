import "./kit.css";

export interface BarItem {
  label: string;
  value: number;
  colorIndex: number;
}

export interface BarChartProps {
  bars: BarItem[];
  formatValue: (n: number) => string;
  className?: string;
}

export function BarChart({
  bars,
  formatValue,
  className = "",
}: BarChartProps) {
  const maxValue = Math.max(...(bars || []).map((b) => b.value), 0);

  return (
    <div className={`kit-barchart ${className}`}>
      {bars.map((bar, i) => {
        const formatted = formatValue(bar.value);
        const pct = maxValue > 0 ? Math.max(3, Math.round((bar.value / maxValue) * 100)) : 0;
        const colorVar = `var(--kit-c${bar.colorIndex % 8})`;

        return (
          <div
            key={i}
            className="kit-barchart-row"
            title={`${bar.label}: ${formatted}`}
            data-bar-label={bar.label}
          >
            <div className="kit-barchart-info">
              <span className="kit-barchart-label">{bar.label}</span>
              <span className="kit-barchart-value">{formatted}</span>
            </div>
            <div className="kit-barchart-track">
              <div
                className="kit-barchart-fill"
                style={{ width: `${pct}%`, backgroundColor: colorVar }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
