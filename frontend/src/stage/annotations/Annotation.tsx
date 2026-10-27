import { useEffect, useRef } from "react";
import { arrowHead, Point, Rect, Size } from "../tourGeometry";
import {
  bracketPaths,
  connectEndpoints,
  connectPath,
  DrawStyle,
  ellipseFor,
  highlightRect,
  lensGeometry,
  pinPos,
  spotlightPath,
  underlinePath,
} from "./drawKit";
import { computePointerTrail, PointerTrailData, setLastPoint } from "./pointerTrail";

const RING_OUTSET = 4;
const ARROW_HEAD_LEN = 20;
const ARROW_SHAFT_MIN = 56;
const ARROW_DESIRED_LEN = ARROW_SHAFT_MIN + ARROW_HEAD_LEN;

export interface AnnotationProps {
  style: DrawStyle;
  target: Rect | null;
  link: Rect | null;
  thumb: string | null;
  viewport: Size;
  live: boolean;
  idx: number;
  orb?: Rect | null;
}

export function Annotation({
  style: drawStyle,
  target,
  link,
  thumb,
  viewport,
  live,
  idx,
  orb,
}: AnnotationProps) {
  const pointerRef = useRef<SVGGElement | null>(null);
  const scanLineRef = useRef<SVGLineElement | null>(null);
  const trailRef = useRef<PointerTrailData | null>(null);

  if (!trailRef.current && target) {
    const rawTrail = computePointerTrail(target, viewport, orb);
    if (rawTrail) {
      const hasRoom = target.x >= 10 && target.y >= 10;
      const parked: Point = {
        x: Math.round(hasRoom ? target.x - 10 : target.x),
        y: Math.round(hasRoom ? target.y - 10 : target.y),
      };
      trailRef.current = {
        start: rawTrail.start,
        end: parked,
        dx: parked.x - rawTrail.start.x,
        dy: parked.y - rawTrail.start.y,
      };
      setLastPoint(parked);
    }
  }

  useEffect(() => {
    const pEl = pointerRef.current;
    const trail = trailRef.current;
    if (pEl && trail) {
      pEl.style.setProperty("--dx", `${trail.dx}px`);
      pEl.style.setProperty("--dy", `${trail.dy}px`);
      const raf = requestAnimationFrame(() => {
        pEl.classList.add("ann-pointer--go");
      });
      return () => cancelAnimationFrame(raf);
    }
  }, []);

  useEffect(() => {
    const sEl = scanLineRef.current;
    if (sEl && target) {
      sEl.style.setProperty("--scan-dist", `${target.h}px`);
    }
  }, [target]);

  if (!target) {
    return <g className="ann-group" />;
  }

  const trail = trailRef.current;

  return (
    <g className="ann-group">
      {/* Pointer trail glides from previous target to current target top-left */}
      {trail && (
        <g
          ref={pointerRef}
          className={`ann-pointer${live ? " in" : ""}`}
        >
          <circle
            className="ann-pointer-ring"
            cx={trail.start.x}
            cy={trail.start.y}
            r={12}
          />
          <circle
            className="ann-pointer-dot"
            cx={trail.start.x}
            cy={trail.start.y}
            r={6}
          />
        </g>
      )}

      {drawStyle === "ring" && (
        <rect
          className="ann ann--ring tour-ring"
          x={target.x - RING_OUTSET}
          y={target.y - RING_OUTSET}
          width={target.w + 2 * RING_OUTSET}
          height={target.h + 2 * RING_OUTSET}
          rx={12}
          pathLength={1}
        />
      )}

      {drawStyle === "underline" && (() => {
        const uPath = underlinePath(target);
        return (
          <>
            <path className="ann ann--underline ann--underline-glow" d={uPath} pathLength={1} />
            <path className="ann ann--underline" d={uPath} pathLength={1} />
          </>
        );
      })()}

      {drawStyle === "bracket" && (() => {
        const b = bracketPaths(target);
        return (
          <>
            <path className="ann ann--bracket ann--bracket-left" d={b.left} pathLength={1} />
            <path className="ann ann--bracket ann--bracket-right" d={b.right} pathLength={1} />
          </>
        );
      })()}

      {drawStyle === "circle" && (() => {
        const el = ellipseFor(target);
        return (
          <ellipse
            className="ann ann--circle"
            cx={el.cx}
            cy={el.cy}
            rx={el.rx}
            ry={el.ry}
            pathLength={1}
          />
        );
      })()}

      {drawStyle === "arrow" && (() => {
        const freeSpace = [
          { side: "left", space: target.x },
          { side: "right", space: viewport.w - (target.x + target.w) },
          { side: "above", space: target.y },
          { side: "below", space: viewport.h - (target.y + target.h) },
        ];
        freeSpace.sort((a, b) => b.space - a.space);
        const arrowSide = freeSpace[0].side;
        const availableSpace = Math.max(0, freeSpace[0].space);
        const arrowLen = Math.min(ARROW_DESIRED_LEN, availableSpace);

        let tip: Point;
        let tail: Point;
        const tcx = Math.round(target.x + target.w / 2);
        const tcy = Math.round(target.y + target.h / 2);

        if (arrowSide === "left") {
          tip = { x: Math.round(target.x), y: tcy };
          tail = { x: Math.round(target.x - arrowLen), y: tcy };
        } else if (arrowSide === "right") {
          tip = { x: Math.round(target.x + target.w), y: tcy };
          tail = { x: Math.round(target.x + target.w + arrowLen), y: tcy };
        } else if (arrowSide === "above") {
          tip = { x: tcx, y: Math.round(target.y) };
          tail = { x: tcx, y: Math.round(target.y - arrowLen) };
        } else {
          tip = { x: tcx, y: Math.round(target.y + target.h) };
          tail = { x: tcx, y: Math.round(target.y + target.h + arrowLen) };
        }

        return (
          <>
            <line
              className="ann ann--arrow ann--arrow-stem"
              x1={tail.x}
              y1={tail.y}
              x2={tip.x}
              y2={tip.y}
              pathLength={1}
            />
            <polygon
              className="ann ann--arrow ann--arrow-head"
              points={arrowHead(tail, tip, ARROW_HEAD_LEN, 8)}
            />
            <circle
              className="ann ann--arrow ann--arrow-dot"
              cx={tip.x}
              cy={tip.y}
              r={4}
            />
          </>
        );
      })()}

      {drawStyle === "highlight" && (() => {
        const hr = highlightRect(target);
        return (
          <rect
            className="ann ann--highlight"
            x={hr.x}
            y={hr.y}
            width={hr.w}
            height={hr.h}
            rx={4}
          />
        );
      })()}

      {drawStyle === "spotlight" && (() => {
        const sp = spotlightPath(viewport, target);
        return (
          <>
            <path className="ann ann--spotlight" d={sp} fillRule="evenodd" />
            <rect
              className="ann ann--spotlight-ring"
              x={target.x}
              y={target.y}
              width={target.w}
              height={target.h}
              rx={8}
            />
          </>
        );
      })()}

      {drawStyle === "scan" && (() => {
        const clipId = `ann-scan-clip-${idx}`;
        return (
          <>
            <clipPath id={clipId}>
              <rect x={target.x} y={target.y} width={target.w} height={target.h} rx={6} />
            </clipPath>
            <rect
              className="ann ann--scan-box"
              x={target.x}
              y={target.y}
              width={target.w}
              height={target.h}
              rx={6}
            />
            <g clipPath={`url(#${clipId})`}>
              <line
                ref={scanLineRef}
                className="ann ann--scan-line"
                x1={target.x}
                y1={target.y}
                x2={target.x + target.w}
                y2={target.y}
              />
            </g>
          </>
        );
      })()}

      {drawStyle === "lens" && (() => {
        const lg = lensGeometry(target, viewport);
        const lensClipId = `ann-lens-clip-${idx}`;
        return (
          <>
            <clipPath id={lensClipId}>
              <circle cx={lg.cx} cy={lg.cy} r={lg.r} />
            </clipPath>
            <line
              className="ann ann--lens ann--lens-connector"
              x1={lg.connectorStart.x}
              y1={lg.connectorStart.y}
              x2={lg.connectorEnd.x}
              y2={lg.connectorEnd.y}
              pathLength={1}
            />
            <circle
              className="ann ann--lens ann--lens-dot"
              cx={lg.connectorStart.x}
              cy={lg.connectorStart.y}
              r={3}
            />
            <circle
              className="ann ann--lens ann--lens-bg"
              cx={lg.cx}
              cy={lg.cy}
              r={lg.r}
            />
            {thumb && (
              <image
                href={thumb}
                x={lg.cx - lg.r}
                y={lg.cy - lg.r}
                width={lg.r * 2}
                height={lg.r * 2}
                preserveAspectRatio="xMidYMid slice"
                clipPath={`url(#${lensClipId})`}
              />
            )}
            <circle
              className="ann ann--lens ann--lens-rim-under"
              cx={lg.cx}
              cy={lg.cy}
              r={lg.r}
            />
            <circle
              className="ann ann--lens ann--lens-rim"
              cx={lg.cx}
              cy={lg.cy}
              r={lg.r}
            />
            <circle
              className="ann ann--lens ann--lens-beam"
              cx={lg.cx}
              cy={lg.cy}
              r={lg.r}
              pathLength={1}
            />
          </>
        );
      })()}

      {drawStyle === "connect" && (() => {
        const cPath = link ? connectPath(target, link) : "";
        const ends = link ? connectEndpoints(target, link) : null;
        return (
          <>
            <rect
              className="ann ann--connect ann--connect-box"
              x={target.x - 3}
              y={target.y - 3}
              width={target.w + 6}
              height={target.h + 6}
              rx={8}
            />
            {link && (
              <rect
                className="ann ann--connect ann--connect-box"
                x={link.x - 3}
                y={link.y - 3}
                width={link.w + 6}
                height={link.h + 6}
                rx={8}
              />
            )}
            {cPath && (
              <>
                <path className="ann ann--connect ann--connect-line" d={cPath} pathLength={1} />
                <path className="ann ann--connect ann--connect-beam" d={cPath} pathLength={1} />
              </>
            )}
            {ends && (
              <>
                <circle
                  className="ann ann--connect ann--connect-dot"
                  cx={ends.from.x}
                  cy={ends.from.y}
                  r={4}
                />
                <circle
                  className="ann ann--connect ann--connect-dot"
                  cx={ends.to.x}
                  cy={ends.to.y}
                  r={4}
                />
              </>
            )}
          </>
        );
      })()}

      {drawStyle === "pin" && (() => {
        const pos = pinPos(target);
        return (
          <g
            className="ann ann--pin"
            transform={`translate(${pos.x}, ${pos.y})`}
          >
            <path
              className="ann-pin-shape"
              d="M 0 0 C -4 -8, -14 -16, -14 -26 A 14 14 0 1 1 14 -26 C 14 -16, 4 -8, 0 0 Z"
            />
            <text
              className="ann-pin-num"
              x={0}
              y={-24}
              textAnchor="middle"
              dominantBaseline="central"
            >
              {idx + 1}
            </text>
          </g>
        );
      })()}
    </g>
  );
}
