import { Section } from "@/components";

const HOURS = 7;

export function Freshness() {
  return (
    <Section id="freshness" className="py-0!">
      <div className="flex max-w-160 flex-col gap-6 px-4 pt-16 md:px-16 md:pt-24">
        <h2 className="text-32 font-medium text-balance md:text-40">
          Under 3 minutes behind the head.{" "}
          <span className="text-muted">
            Derived Solana tables elsewhere trail by an hour or more.
          </span>
        </h2>
        <p className="font-mono text-12 text-muted">
          Vendor figures from their published documentation.
        </p>
      </div>

      <div
        role="img"
        aria-label="How far behind the chain head a derived Solana table sits: Amp under 3 minutes, Allium 1 hour, Dune 6 to 7 hours."
        className="flex flex-col gap-3 px-4 pt-16 pb-16 md:px-16 md:pb-24"
      >
        <div className="flex h-5 font-mono text-12 text-muted tabular-nums">
          {Array.from({ length: HOURS + 1 }, (_, h) => (
            <span
              key={h}
              className={`absolute ${
                h === 0
                  ? ""
                  : h === HOURS
                  ? "-translate-x-full"
                  : "-translate-x-1/2"
              } ${h === 0 ? "text-brand-800" : ""}`}
              style={{ left: `${(h / HOURS) * 100}%` }}
            >
              {h === 0 ? "chain head" : `${h}h`}
            </span>
          ))}
        </div>
        <div className="flex flex-col gap-10 border-e border-subtle bg-[linear-gradient(to_right,var(--border-color-subtle)_1px,transparent_1px)] bg-size-[calc(100%/7)_100%] bg-repeat-x py-6">
          {LANES.map((l) => (
            <div key={l.name} className="flex flex-col gap-2">
              <span className="flex flex-wrap items-baseline gap-x-3 text-14">
                <span
                  className={`font-medium ${l.amp ? "text-brand-500" : ""}`}
                >
                  {l.name}
                </span>
                <span
                  className={`font-mono tabular-nums ${
                    l.amp ? "text-brand-500" : ""
                  }`}
                >
                  {l.label}
                </span>
              </span>
              <span className="flex h-6">
                <span
                  className={`h-full min-w-1.5 ${
                    l.amp ? "bg-brand-500" : "bg-muted "
                  }`}
                  style={{ width: `${(l.from / (HOURS * 60)) * 100}%` }}
                />
                {/* {l.to ? (
                  <span
                    className="h-full border border-s-0 border-dashed border-default"
                    style={{
                      width: `${((l.to - l.from) / (HOURS * 60)) * 100}%`,
                    }}
                  />
                ) : null} */}
              </span>
              <span className="text-12 text-muted">{l.note}</span>
            </div>
          ))}
        </div>
      </div>
    </Section>
  );
}

const LANES = [
  {
    name: "Amp",
    label: "under 3 minutes",
    note: "",
    from: 3,
    amp: true,
  },
  {
    name: "Allium",
    label: "1 hour",
    note: "Documented for most enriched historical tables",
    from: 60,
  },
  {
    name: "Dune",
    label: "6 to 7 hours",
    note: "Documented for Solana DEX trades and token transfers tables",
    from: 360,
    to: 420,
  },
];
