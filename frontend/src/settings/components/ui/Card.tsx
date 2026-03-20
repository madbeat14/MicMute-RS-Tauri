import { ReactNode } from "react";

interface CardProps {
  title: string;
  icon: string;
  className?: string;
  children: ReactNode;
}

export function Card({ title, icon, className = "", children }: CardProps) {
  return (
    <section className={`card ${className}`}>
      <div className="card-header">
        <span>{icon} {title}</span>
      </div>
      <div className="card-body">{children}</div>
    </section>
  );
}
