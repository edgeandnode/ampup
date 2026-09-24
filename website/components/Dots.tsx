interface Props {
  corner?: "tl" | "tr" | "bl" | "br";
  className?: string;
}

export function Dots({ corner = "tr", className }: Props) {
  const at = { tl: "0% 0%", tr: "100% 0%", bl: "0% 100%", br: "100% 100%" }[
    corner
  ];
  const fade = `radial-gradient(120% 120% at ${at}, black 0%, transparent 62%)`;
  return (
    <div
      aria-hidden
      className={`pointer-events-none absolute inset-0 text-brand-500/10 ${className}`}
      style={{
        backgroundImage:
          "radial-gradient(currentColor 1.2px, transparent 1.8px)",
        backgroundSize: "10px 10px",
        backgroundRepeat: "repeat",
        backgroundPosition: "0 0",
        WebkitMaskImage: fade,
        maskImage: fade,
        WebkitMaskSize: "100% 100%",
        maskSize: "100% 100%",
      }}
    />
  );
}
