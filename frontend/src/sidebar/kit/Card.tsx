import type { ReactNode } from "react";
import "./kit.css";

export interface CardProps {
  title?: ReactNode;
  trailing?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function Card({
  title,
  trailing,
  children,
  className = "",
}: CardProps) {
  return (
    <section className={`kit-card ${className}`}>
      {(title || trailing) && (
        <div className="kit-card-header">
          {title && (
            <h2 className="kit-card-title">
              {title}
            </h2>
          )}
          {trailing && <div className="kit-card-trailing">{trailing}</div>}
        </div>
      )}
      <div className="kit-card-body">{children}</div>
    </section>
  );
}
