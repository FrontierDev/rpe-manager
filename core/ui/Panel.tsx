import type { HTMLAttributes, PropsWithChildren } from "react";

type PanelProps = PropsWithChildren<HTMLAttributes<HTMLElement>>;

export function Panel({ children, className = "", ...props }: PanelProps) {
  return (
    <section className={`ui-panel ${className}`.trim()} {...props}>
      {children}
    </section>
  );
}
