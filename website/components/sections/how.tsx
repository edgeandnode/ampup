import { useReducedMotion } from "motion/react";
import { AmpLogoIcon } from "@edgeandnode/eds-react/icons";
import {
  NetworkArbitrumOne,
  NetworkBase,
  NetworkEthereum,
  NetworkOptimism,
  NetworkPolygon,
  NetworkSolana,
} from "@web3icons/react";
import { Reveal, Section } from "@/components";
import { INTEGRATIONS } from "@/data/integrations";
import { CodeInline } from "@edgeandnode/eds-react";

export function How() {
  const reduce = useReducedMotion();
  return (
    <Section id="how-alt" className="flex flex-col gap-12">
      <Reveal className="flex max-w-200 flex-col gap-4 px-4 md:px-16">
        <h2 className="text-32 font-medium text-balance md:text-40">
          Pull any chain into the tools you already run.
        </h2>
        <p className="text-18 text-muted">
          Amp extracts every block, transaction, and log from the chains you
          point it at, keeps them as Parquet in your bucket, and serves them as
          SQL to whatever you query with today.
        </p>
      </Reveal>
      <Reveal>
        <div className="dotgrid border-y border-subtle px-4 py-10 md:px-16">
          <div className="relative aspect-1000/440 w-full max-lg:hidden">
            <svg
              viewBox="0 0 1000 440"
              aria-hidden
              className="absolute inset-0 size-full overflow-visible"
            >
              <defs>
                <filter
                  id="beam-glow"
                  x="-50%"
                  y="-50%"
                  width="200%"
                  height="200%"
                >
                  <feGaussianBlur stdDeviation="1" />
                </filter>
              </defs>
              {ROWS.map((y, i) => (
                <g key={i}>
                  {[inPath(y), outPath(y)].map((d, k) => (
                    <g key={k}>
                      <path
                        d={d}
                        fill="none"
                        stroke="var(--border-color-default)"
                        strokeWidth="1"
                      />
                      {reduce ? null : <Beam d={d} begin={k * 1.2 + i * 0.4} />}
                    </g>
                  ))}
                </g>
              ))}
            </svg>

            <span className="absolute top-0 left-[29.5%] -translate-x-1/2 font-mono text-12 text-brand-800">
              extract and verify
            </span>
            <span className="absolute top-0 left-[70.5%] -translate-x-1/2 font-mono text-12 text-brand-800">
              query or stream
            </span>

            {CHAINS.map(([Icon, name], i) => (
              <div
                key={name}
                className="absolute left-0 flex h-11 w-[19%] -translate-y-1/2 items-center gap-2.5 border border-default bg-canvas px-3 text-14"
                style={{ top: `${(ROWS[i]! / 440) * 100}%` }}
              >
                <Icon variant="branded" size={20} aria-hidden />
                {name}
              </div>
            ))}

            <div className="absolute top-[34%] left-[40%] flex h-[32%] w-[20%] flex-col border border-default bg-canvas">
              <span
                aria-hidden
                className="absolute inset-x-0 -top-px h-0.5 bg-brand-500"
              />
              <span className="flex items-center gap-2 border-b border-subtle px-4 py-3 text-18 font-medium">
                <AmpLogoIcon alt="" variant="branded" size={6} />
                Amp
              </span>
              <span className="flex flex-1 flex-col justify-center gap-1 px-4 font-mono text-12 text-muted">
                <span>Parquet in your bucket</span>
                <span>SQL over Arrow Flight</span>
              </span>
            </div>

            {TOOLS.map((t, i) => (
              <div
                key={t.name}
                className="absolute right-0 flex h-11 w-[19%] -translate-y-1/2 items-center gap-2.5 border border-default bg-canvas px-3 text-14"
                style={{ top: `${(ROWS[i]! / 440) * 100}%` }}
              >
                <svg
                  viewBox="0 0 24 24"
                  aria-hidden
                  className="size-5 text-muted"
                >
                  <path d={t.path} fill="currentColor" />
                </svg>
                {t.name}
              </div>
            ))}
          </div>

          <div className="flex flex-col items-center gap-4 lg:hidden">
            <div className="flex flex-wrap justify-center gap-2">
              {CHAINS.map(([Icon, name]) => (
                <span
                  key={name}
                  className="flex items-center gap-2 border border-default bg-canvas px-3 py-1.5 text-14"
                >
                  <Icon variant="branded" size={16} aria-hidden />
                  {name}
                </span>
              ))}
            </div>
            <span className="font-mono text-12 text-brand-800">
              ↓ extract and verify
            </span>
            <div className="flex items-center gap-2 border border-default border-t-2 border-t-brand-500 bg-canvas px-5 py-3 text-18 font-medium">
              <AmpLogoIcon alt="" variant="branded" size={6} />
              Amp
            </div>
            <span className="font-mono text-12 text-brand-800">
              ↓ query or stream
            </span>
            <div className="flex flex-wrap justify-center gap-2">
              {TOOLS.map((t) => (
                <span
                  key={t.name}
                  className="flex items-center gap-2 border border-default bg-canvas px-3 py-1.5 text-14"
                >
                  <svg
                    viewBox="0 0 24 24"
                    aria-hidden
                    className="size-4 text-muted"
                  >
                    <path d={t.path} fill="currentColor" />
                  </svg>
                  {t.name}
                </span>
              ))}
            </div>
          </div>
        </div>
        <div className="grid gap-8 md:grid-cols-3 px-4 md:px-16">
          {VERBS.map(([verb, rest]) => (
            <p
              key={verb}
              className="border-t border-elevated -mt-px pt-8 text-16"
            >
              <CodeInline>{verb}</CodeInline> {rest}
            </p>
          ))}
        </div>
      </Reveal>
    </Section>
  );
}

// A light beam along a path: a bright head and a blurred trail, both dashes
// on a path normalised to length 1. Every layer's leading edge travels from
// the start to past the end (1.22, the longest trail) in the same time.
function Beam({ d, begin }: { d: string; begin: number }) {
  const run = (len: number, extra = {}) => (
    <path
      d={d}
      fill="none"
      pathLength={0.5}
      strokeDasharray={`${len} 2`}
      strokeDashoffset={len}
      strokeLinecap="round"
      className="stroke-brand-500/40"
      {...extra}
    >
      <animate
        attributeName="stroke-dashoffset"
        from={len}
        to={len - 1.22}
        dur="2.4s"
        begin={`${begin}s`}
        repeatCount="indefinite"
      />
    </path>
  );
  return (
    <>
      {run(0.22, {
        strokeWidth: 4,
        strokeOpacity: 0.35,
        filter: "url(#beam-glow)",
      })}
      {run(0.14, { strokeWidth: 1.5, strokeOpacity: 0.5 })}
      {run(0.04, { strokeWidth: 2 })}
    </>
  );
}

// Row centres in the 1000 by 440 drawing. Amp sits at the middle, x 400 to 600.
const ROWS = [40, 112, 184, 256, 328, 400];
const inPath = (y: number) => `M190 ${y} C300 ${y} 300 220 400 220`;
const outPath = (y: number) => `M600 220 C700 220 700 ${y} 810 ${y}`;

const CHAINS = [
  [NetworkEthereum, "Ethereum"],
  [NetworkBase, "Base"],
  [NetworkSolana, "Solana"],
  [NetworkArbitrumOne, "Arbitrum"],
  [NetworkOptimism, "Optimism"],
  [NetworkPolygon, "Polygon"],
] as const;

const TOOLS = [
  "Snowflake",
  "ClickHouse",
  "Postgres",
  "Databricks",
  "Apache Kafka",
  "Grafana",
].map((name) => {
  const it = INTEGRATIONS.flatMap((g) => g.items).find((x) => x.name === name)!;
  return { name, path: it.path! };
});

const VERBS = [
  [
    "extract",
    "every block, transaction, and log from the chains you name, verified against their own hashes",
  ],
  [
    "query",
    "with standard SQL over Arrow Flight, decoding ABI, Borsh, SCALE, and XDR at query time",
  ],
  [
    "stream",
    "any query as microbatches that replay on reorg and resume from a cursor",
  ],
];
