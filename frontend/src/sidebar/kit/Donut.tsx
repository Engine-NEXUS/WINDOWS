import "./kit.css";

export interface DonutSlice {
  label: string;
  value: number;
  colorIndex: number;
}

export interface DonutProps {
  slices: DonutSlice[];
  centerTop: string;
  centerBottom: string;
  size?: number; // default 140
  className?: string;
}

export function Donut({
  slices,
  centerTop,
  centerBottom,
  size = 140,
  className = "",
}: DonutProps) {
  // Hard rule: Zero-value slices are skipped
  const validSlices = (slices || []).filter((s) => typeof s.value === "number" && s.value > 0);
  const totalValue = validSlices.reduce((sum, s) => sum + s.value, 0);

  const cx = size / 2;
  const cy = size / 2;
  const strokeWidth = 14;
  const radius = Math.max(10, cx - strokeWidth - 4);
  const circumference = 2 * Math.PI * radius;

  let accumulatedLength = 0;
  const sliceCount = validSlices.length;

  return (
    <div className={`kit-donut ${className}`} style={{ width: size, height: size }}>
      <svg
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        className="kit-donut-svg"
      >
        {totalValue === 0 ? (
          <circle
            cx={cx}
            cy={cy}
            r={radius}
            fill="none"
            stroke="var(--hub-card-2)"
            strokeWidth={strokeWidth}
          />
        ) : (
          validSlices.map((slice, i) => {
            const fraction = slice.value / totalValue;
            const angleDegrees = fraction * 360;
            const arcLength = fraction * circumference;
            const gap = sliceCount > 1 ? 3 : 0;
            const dashVisible = Math.max(0.5, arcLength - gap);
            const dashGap = circumference - dashVisible;

            const offset = -accumulatedLength;
            accumulatedLength += arcLength;

            const colorVar = `var(--kit-c${slice.colorIndex % 8})`;

            return (
              <circle
                key={i}
                data-slice="true"
                data-label={slice.label}
                data-value={slice.value}
                data-angle={angleDegrees}
                cx={cx}
                cy={cy}
                r={radius}
                fill="none"
                stroke={colorVar}
                strokeWidth={strokeWidth}
                strokeDasharray={`${dashVisible} ${dashGap}`}
                strokeDashoffset={offset}
                strokeLinecap="round"
                transform={`rotate(-90 ${cx} ${cy})`}
              >
                <title>{`${slice.label}: ${slice.value}`}</title>
              </circle>
            );
          })
        )}

        {/* Center labels */}
        <text
          x={cx}
          y={cy - 3}
          textAnchor="middle"
          className="kit-donut-center-top"
        >
          {centerTop}
        </text>
        <text
          x={cx}
          y={cy + 13}
          textAnchor="middle"
          className="kit-donut-center-bottom"
        >
          {centerBottom}
        </text>
      </svg>
    </div>
  );
}
