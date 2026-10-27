import { useMemo, useState } from "react";

interface ContributionDay {
  date: string;
  dayOfWeek: number; // 0 = Sun, 1 = Mon, ..., 6 = Sat
  count: number;
  level: 0 | 1 | 2 | 3 | 4;
  monthName?: string;
  formattedDate: string;
}

// Generate realistic contribution data for the past 24 weeks ending Oct 18, 2026
function generateContributions(): { weeks: ContributionDay[][]; totalCount: number; currentStreak: number; longestStreak: number } {
  const weeks: ContributionDay[][] = [];
  const today = new Date(2026, 9, 18); // Oct 18, 2026 (Month is 0-indexed: 9 = Oct)

  // 24 weeks = 168 days
  const totalDays = 24 * 7;
  const startDate = new Date(today);
  startDate.setDate(today.getDate() - totalDays + 1);

  // Align start to preceding Sunday
  const startDay = startDate.getDay();
  startDate.setDate(startDate.getDate() - startDay);

  let currentWeek: ContributionDay[] = [];
  let totalCount = 0;
  let lastMonth = -1;

  // Realistic seed pattern with heavy activity in recent weeks (Ultron development)
  const cursor = new Date(startDate);
  while (cursor <= today) {
    const dayOfWeek = cursor.getDay();
    const isWeekend = dayOfWeek === 0 || dayOfWeek === 6;
    const month = cursor.getMonth();

    // Determine commit probability and count
    const timeDiff = today.getTime() - cursor.getTime();
    const weeksAgo = Math.floor(timeDiff / (7 * 24 * 60 * 60 * 1000));
    
    // Higher activity in the last 6 weeks (Ultron launch push)
    const recencyMultiplier = weeksAgo < 6 ? 1.8 : 1.0;
    const baseRand = Math.random();
    
    let count = 0;
    if (isWeekend) {
      if (baseRand > 0.45) count = Math.floor(Math.random() * 4 * recencyMultiplier);
    } else {
      if (baseRand > 0.15) count = Math.floor((1 + Math.random() * 8) * recencyMultiplier);
    }

    // Ensure active streak for last 14 days
    if (weeksAgo <= 2 && count === 0) {
      count = Math.floor(2 + Math.random() * 4);
    }

    let level: 0 | 1 | 2 | 3 | 4 = 0;
    if (count > 0 && count <= 2) level = 1;
    else if (count >= 3 && count <= 5) level = 2;
    else if (count >= 6 && count <= 8) level = 3;
    else if (count >= 9) level = 4;

    totalCount += count;

    let monthName: string | undefined = undefined;
    if (month !== lastMonth && cursor.getDate() <= 7) {
      monthName = cursor.toLocaleDateString("en-US", { month: "short" });
      lastMonth = month;
    }

    const dayObj: ContributionDay = {
      date: cursor.toISOString().split("T")[0],
      dayOfWeek,
      count,
      level,
      monthName,
      formattedDate: cursor.toLocaleDateString("en-US", {
        weekday: "short",
        month: "short",
        day: "numeric",
        year: "numeric",
      }),
    };

    currentWeek.push(dayObj);

    if (dayOfWeek === 6) {
      weeks.push(currentWeek);
      currentWeek = [];
    }

    cursor.setDate(cursor.getDate() + 1);
  }

  if (currentWeek.length > 0) {
    weeks.push(currentWeek);
  }

  return {
    weeks,
    totalCount: totalCount + 180, // Realistic total around 640-700
    currentStreak: 14,
    longestStreak: 29,
  };
}

export function GitHubContributionGraph() {
  const [hoveredDay, setHoveredDay] = useState<ContributionDay | null>(null);

  const { weeks, totalCount } = useMemo(() => generateContributions(), []);

  return (
    <div className="gh-graph-card">
      {/* ── Graph Header (Clean Count & Hover Tooltip — No Title, No Streak) ── */}
      <div className="gh-graph-header">
        {hoveredDay ? (
          <span className="gh-graph-tooltip-text">
            <strong>{hoveredDay.count} contribution{hoveredDay.count === 1 ? "" : "s"}</strong> on {hoveredDay.formattedDate}
          </span>
        ) : (
          <span className="gh-graph-tooltip-placeholder">
            {totalCount} contributions in 2026
          </span>
        )}
      </div>

      {/* ── Month Row Labels ── */}
      <div className="gh-graph-months-row">
        <span className="gh-graph-month-label">May</span>
        <span className="gh-graph-month-label">Jun</span>
        <span className="gh-graph-month-label">Jul</span>
        <span className="gh-graph-month-label">Aug</span>
        <span className="gh-graph-month-label">Sep</span>
        <span className="gh-graph-month-label">Oct</span>
      </div>

      {/* ── Grid Container (Days on Y, Weeks on X) ── */}
      <div className="gh-graph-matrix-wrap">
        {/* Day of Week Labels */}
        <div className="gh-graph-day-labels">
          <span className="gh-graph-day-label">Mon</span>
          <span className="gh-graph-day-label">Wed</span>
          <span className="gh-graph-day-label">Fri</span>
        </div>

        {/* Heatmap Columns */}
        <div className="gh-graph-grid">
          {weeks.map((week, wIdx) => (
            <div key={wIdx} className="gh-graph-week-col">
              {week.map((day) => (
                <div
                  key={day.date}
                  className={`gh-graph-cell gh-graph-cell--lvl-${day.level}`}
                  onMouseEnter={() => setHoveredDay(day)}
                  onMouseLeave={() => setHoveredDay(null)}
                  title={`${day.count} contributions on ${day.formattedDate}`}
                />
              ))}
            </div>
          ))}
        </div>
      </div>

      {/* ── Bottom Legend ── */}
      <div className="gh-graph-footer">
        <span className="gh-graph-footer-sub">Continuous daily deployment</span>
        <div className="gh-graph-legend">
          <span className="gh-legend-label">Less</span>
          <span className="gh-legend-cell gh-graph-cell--lvl-0" />
          <span className="gh-legend-cell gh-graph-cell--lvl-1" />
          <span className="gh-legend-cell gh-graph-cell--lvl-2" />
          <span className="gh-legend-cell gh-graph-cell--lvl-3" />
          <span className="gh-legend-cell gh-graph-cell--lvl-4" />
          <span className="gh-legend-label">More</span>
        </div>
      </div>
    </div>
  );
}
