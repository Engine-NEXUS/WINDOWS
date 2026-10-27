import { useMemo } from "react";

interface ActivityBreakdown {
  commitsPercent: number;
  pullRequestsPercent: number;
  codeReviewPercent: number;
  issuesPercent: number;
}

export function GitHubActivityRadarGraph({
  data = {
    commitsPercent: 92,
    pullRequestsPercent: 7,
    codeReviewPercent: 0,
    issuesPercent: 1,
  },
}: {
  data?: ActivityBreakdown;
}) {
  // SVG Dimensions & Center
  const width = 280;
  const height = 220;
  const cx = 145;
  const cy = 110;
  const axisLength = 70; // Maximum radius for 100%

  // Calculate coordinates for the 4 data points
  // Left: Commits (x decreasing)
  // Right: Issues (x increasing)
  // Top: Code review (y decreasing)
  // Bottom: Pull requests (y increasing)
  const coords = useMemo(() => {
    const leftDist = (data.commitsPercent / 100) * axisLength;
    const bottomDist = Math.max(3, (data.pullRequestsPercent / 100) * axisLength);
    const rightDist = Math.max(2, (data.issuesPercent / 100) * axisLength);
    const topDist = Math.max(2, (data.codeReviewPercent / 100) * axisLength);

    const pLeft = { x: cx - leftDist, y: cy };
    const pBottom = { x: cx, y: cy + bottomDist };
    const pRight = { x: cx + rightDist, y: cy };
    const pTop = { x: cx, y: cy - topDist };

    const pointsString = `${pTop.x},${pTop.y} ${pRight.x},${pRight.y} ${pBottom.x},${pBottom.y} ${pLeft.x},${pLeft.y}`;

    return { pTop, pRight, pBottom, pLeft, pointsString };
  }, [data, cx, cy, axisLength]);

  return (
    <div className="gh-graph-card gh-radar-card">
      <svg
        className="gh-radar-svg"
        viewBox={`0 0 ${width} ${height}`}
        width="100%"
        height="100%"
        preserveAspectRatio="xMidYMid meet"
      >
        {/* ── 4 Orthogonal Green Axis Lines ── */}
        {/* Horizontal Axis: Commits (Left) <---> Issues (Right) */}
        <line
          x1={cx - axisLength}
          y1={cy}
          x2={cx + axisLength}
          y2={cy}
          className="gh-radar-axis-line"
        />

        {/* Vertical Axis: Code Review (Top) <---> Pull Requests (Bottom) */}
        <line
          x1={cx}
          y1={cy - axisLength}
          x2={cx}
          y2={cy + axisLength}
          className="gh-radar-axis-line"
        />

        {/* ── Shaded Activity Polygon Area ── */}
        <polygon
          points={coords.pointsString}
          className="gh-radar-polygon"
        />

        {/* ── 4 Vertex Point Indicators (Glowing Circles with White Center) ── */}
        {/* Left vertex (Commits) */}
        <circle cx={coords.pLeft.x} cy={coords.pLeft.y} r="3.5" className="gh-radar-dot" />

        {/* Bottom vertex (Pull requests) */}
        <circle cx={coords.pBottom.x} cy={coords.pBottom.y} r="3" className="gh-radar-dot" />

        {/* Right vertex (Issues) */}
        <circle cx={coords.pRight.x} cy={coords.pRight.y} r="2.5" className="gh-radar-dot" />

        {/* Top vertex (Code review) */}
        <circle cx={coords.pTop.x} cy={coords.pTop.y} r="2.5" className="gh-radar-dot" />

        {/* ── 4 Axis Labels matching Screenshot ── */}
        {/* Top: Code review */}
        <text
          x={cx}
          y={cy - axisLength - 12}
          textAnchor="middle"
          className="gh-radar-label gh-radar-label--top"
        >
          Code review
        </text>

        {/* Bottom: 7% Pull requests */}
        <text
          x={cx}
          y={cy + axisLength + 16}
          textAnchor="middle"
          className="gh-radar-label gh-radar-label--bottom"
        >
          <tspan x={cx} dy="0" className="gh-radar-percent">
            {data.pullRequestsPercent}%
          </tspan>
          <tspan x={cx} dy="13" className="gh-radar-name">
            Pull requests
          </tspan>
        </text>

        {/* Left: 92% Commits */}
        <text
          x={cx - axisLength - 14}
          y={cy - 4}
          textAnchor="end"
          className="gh-radar-label gh-radar-label--left"
        >
          <tspan x={cx - axisLength - 14} dy="0" className="gh-radar-percent">
            {data.commitsPercent}%
          </tspan>
          <tspan x={cx - axisLength - 14} dy="13" className="gh-radar-name">
            Commits
          </tspan>
        </text>

        {/* Right: 1% Issues */}
        <text
          x={cx + axisLength + 14}
          y={cy - 4}
          textAnchor="start"
          className="gh-radar-label gh-radar-label--right"
        >
          <tspan x={cx + axisLength + 14} dy="0" className="gh-radar-percent">
            {data.issuesPercent}%
          </tspan>
          <tspan x={cx + axisLength + 14} dy="13" className="gh-radar-name">
            Issues
          </tspan>
        </text>
      </svg>
    </div>
  );
}
