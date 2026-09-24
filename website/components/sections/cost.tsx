import { Section } from "@/components";

export function Cost() {
  return (
    <Section id="cost" className="py-0! overflow-clip">
      <div className="flex max-w-160 flex-col gap-12 px-4 pt-16 pb-80 md:px-16 md:pt-24 lg:min-h-160 lg:pb-24">
        <h2 className="text-32 font-medium text-balance md:text-40">
          Scale usage without scaling the bill.{" "}
          <span className="text-muted">
            A flat license, with queries that are never metered.
          </span>
        </h2>
        <figcaption className="flex flex-wrap gap-x-6 gap-y-2 text-12 text-muted">
          <span className="flex items-center gap-2">
            <span aria-hidden className="size-3 rounded-2 bg-space-600" />
            Pay per query or compute credits
          </span>
          <span className="flex items-center gap-2">
            <span aria-hidden className="size-3 rounded-2 bg-brand-default" />
            Amp, flat license
          </span>
        </figcaption>
        {/* <dl className="grid max-w-120 grid-cols-2 gap-x-10 gap-y-8">
          {STATS.map(([value, label]) => (
            <div
              key={label}
              className="flex flex-col gap-1 border-s-2 border-brand-500 ps-4"
            >
              <dt className="text-32 font-medium tabular-nums">{value}</dt>
              <dd className="text-14 text-muted">{label}</dd>
            </div>
          ))}
        </dl> */}
      </div>

      <div
        role="img"
        aria-label="Cost against queries: pay per query keeps climbing, Amp's flat license stays level past the breakeven point."
        className="pointer-events-none absolute inset-x-0 bottom-0 h-72 lg:inset-y-0 lg:h-auto"
      >
        <div
          className="absolute inset-0 bg-[linear-gradient(to_bottom,color-mix(in_oklab,var(--color-brand-500)_8%,transparent),transparent),repeating-linear-gradient(to_right,var(--border-color-subtle)_0_1px,transparent_1px_10px)]"
          style={{ clipPath: `polygon(${AREA})` }}
        />
        <svg
          viewBox="0 0 100 100"
          preserveAspectRatio="none"
          aria-hidden
          className="absolute inset-0 size-full overflow-visible"
        >
          <path
            d={CURVE}
            fill="none"
            className="stroke-space-600"
            strokeWidth="2"
            vectorEffect="non-scaling-stroke"
          />
          <path
            d={`M0 ${FLAT} H100`}
            fill="none"
            className="stroke-brand-500"
            strokeWidth="2"
            vectorEffect="non-scaling-stroke"
          />
        </svg>
        <span
          className="absolute size-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-brand-500 bg-canvas"
          style={{ left: `${BREAKEVEN}%`, top: `${FLAT}%` }}
        />
        <span
          className="absolute -translate-y-full pb-2 font-mono text-12 text-muted"
          style={{ left: `calc(${BREAKEVEN}% + 12px)`, top: `calc(${FLAT}% + 40px)` }}
        >
          breakeven
        </span>
        <span
          className="absolute inset-e-4 font-mono text-12 text-muted"
          style={{ top: `${FLAT}%` }}
        >
          queries
        </span>
      </div>
    </Section>
  );
}

// Pay-per-query cost as a cubic in a 100 by 100 box, y measured from the top.
const P = [
  [0, 95],
  [45, 95],
  [72, 58],
  [100, 4],
] as const;
const CURVE = `M${P[0].join(" ")} C${P[1].join(" ")} ${P[2].join(
  " ",
)} ${P[3].join(" ")}`;
const AREA = Array.from({ length: 49 }, (_, i) => {
  const t = i / 48;
  const u = 1 - t;
  const at = (k: 0 | 1) =>
    u ** 3 * P[0][k] +
    3 * u * u * t * P[1][k] +
    3 * u * t * t * P[2][k] +
    t ** 3 * P[3][k];
  return `${at(0).toFixed(2)}% ${at(1).toFixed(2)}%`;
})
  .concat("100% 100%", "0% 100%")
  .join(", ");
const FLAT = 80;
const BREAKEVEN = 44.6;

const STATS = [
  ["Flat", "license fee, agreed up front"],
  ["0", "charges per query or per dataset"],
  ["5×", "smaller at rest than BigQuery"],
  ["Yours", "hardware, in your cloud or on-prem"],
];
