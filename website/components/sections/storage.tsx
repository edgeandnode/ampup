import { Section } from "@/components";

// Squares drawn to scale by area: side = sqrt(159 / 800).
const SIDE = `${(Math.sqrt(159 / 800) * 100).toFixed(1)}%`;

export function Storage() {
  return (
    <Section id="storage" className="py-0!">
      <div className="grid lg:grid-cols-2">
        <div className="flex flex-col justify-between gap-12 px-4 py-16 md:px-16 md:py-24">
          <h2 className="text-32 font-medium text-balance md:text-40">
            5× smaller at rest.{" "}
            <span className="text-muted">
              Raw data stays encoded and decodes at query time.
            </span>
          </h2>
          <div className="flex flex-col gap-3">
            <p className="max-w-100 text-16 text-muted">
              Amp keeps raw blockchain data in compact encoded form instead of
              expanding it into wide tables. The same Solana history takes a
              fifth of the space.
            </p>
            <p className="font-mono text-12 text-muted">
              Public Solana tables, same coverage window.
            </p>
          </div>
        </div>

        <div className="flex items-center justify-center border-subtle px-4 py-12 max-lg:border-t md:p-16 lg:border-s">
          <div
            role="img"
            aria-label="Squares to scale by area: BigQuery about 800 TB, Amp about 159 TB."
            className="aspect-square w-full max-w-120 border border-default bg-[repeating-linear-gradient(to_right,var(--border-color-subtle)_0_1px,transparent_1px_10px)]"
          >
            <span className="absolute top-4 end-4 flex flex-col items-end font-mono text-12">
              <span className="text-muted">BigQuery</span>
              <span className="text-24 text-default tabular-nums">~800 TB</span>
            </span>
            <div
              className="absolute bottom-0 left-0 aspect-square border-2 border-brand-500 bg-[color-mix(in_oklab,var(--color-brand-500)_14%,var(--background-color-canvas))]"
              style={{ width: SIDE }}
            >
              <span className="absolute top-3 left-3 flex flex-col font-mono text-12">
                <span className="text-brand-800">Amp</span>
                <span className="text-24 text-brand-800 tabular-nums">
                  ≈159 TB
                </span>
              </span>
            </div>
          </div>
        </div>
      </div>
    </Section>
  );
}
