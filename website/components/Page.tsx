import { Button, ButtonGroup, Link } from "@edgeandnode/eds-react";
import { AmpLogoIcon } from "@edgeandnode/eds-react/icons";
import {
  AnimatePresence,
  LazyMotion,
  domMax,
  m,
  useReducedMotion,
} from "motion/react";
import { LINKS } from "@/data/constants";
import { useHeads } from "@/lib/use-heads";
import {
  Hero,
  LiveSection,
  CTA,
  Engine,
  Features,
  How,
  Integrations,
  Proof,
  Cost,
  Storage,
  Freshness,
  Try,
  Docs,
  License,
} from "./sections";
import { NetworkEthereum } from "@web3icons/react";

export default function Page() {
  return (
    <div className="min-h-dvh bg-canvas font-sans text-default">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 mx-auto max-w-280 border-x border-subtle max-lg:hidden"
      />
      <header className="sticky top-0 z-10 border-b border-subtle bg-canvas">
        <div className="flex h-16 items-center justify-between gap-6 mx-auto max-w-280 ">
          <div className="flex items-center gap-2">
            <AmpLogoIcon alt="" variant="branded" size={7} />
            <span className="text-20 font-medium">Amp</span>
          </div>
          <div className="flex items-center gap-6">
            <Link href={LINKS.docs} variant="secondary" className="text-14">
              Docs
            </Link>
            <Button variant="primary" size="small" href={LINKS.contact}>
              Talk to the team
            </Button>
          </div>
        </div>
      </header>

      <main className="">
        <Hero />
        <PatternBlocks />
        <LiveSection />
        <PatternHashes />
        <How />

        <Features />
        <PatternHatch />
        {/* <Proof /> */}
        <Storage />
        <Freshness />
        <Cost />
        <Engine />
        <Integrations />
        <License />
        <Try />
        <PatternHatch />
        <Docs />
        <PatternHatch />
        <CTA />
      </main>

      <footer className="flex flex-wrap items-center justify-between gap-4 py-8 text-12 text-muted">
        <span>
          Amp is built by Edge & Node, the team behind The Graph. Published
          under BUSL-1.1.
        </span>
        <span className="flex gap-4">
          <Link href={LINKS.edgeandnode} variant="secondary">
            Edge &amp; Node
          </Link>
        </span>
      </footer>
    </div>
  );
}

const Band = ({ children }: { children?: React.ReactNode }) => (
  <div
    aria-hidden="true"
    className="mx-auto -mt-8 h-16 max-w-280 overflow-clip border border-subtle bg-canvas"
  >
    {children}
  </div>
);

const LINE = "var(--border-color-default)";

const PatternHatch = () => (
  <Band>
    <div
      className="size-full"
      style={{
        backgroundImage:
          "repeating-linear-gradient(135deg, transparent, transparent 10px, #e6e7eb 10px, #e6e7eb 11px)",
      }}
    />
  </Band>
);

const PatternBlocks = () => {
  const head = useHeads().find((h) => h.network === "base")!.ampHead;
  const blocks = Array.from({ length: 48 }, (_, i) => head - 47 + i);
  return (
    <Band>
      <LazyMotion features={domMax}>
        <div
          className="flex size-full items-center justify-end gap-3 px-6"
          style={{
            maskImage: "linear-gradient(to right, transparent, black 30%)",
            maskSize: "100% 100%",
          }}
        >
          <AnimatePresence initial={false} mode="popLayout">
            {blocks.map((n) => (
              <m.span
                key={n}
                layout
                initial={{ opacity: 0, scale: 0.4 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.4, ease: [0.25, 0.1, 0.25, 1] }}
                className={`size-3 shrink-0 rounded-2 border ${
                  n === head
                    ? "border-brand-500 bg-brand-500"
                    : "border-default bg-canvas"
                }`}
              />
            ))}
          </AnimatePresence>
          <div className="flex gap-1 shrink-0">
            <NetworkEthereum size={16} variant="branded" />
            <span className="shrink-0 ps-1 font-mono text-12 text-brand-500 tabular-nums">
              {head.toLocaleString("en-US")}
            </span>
          </div>
        </div>
      </LazyMotion>
    </Band>
  );
};

const PatternHashes = () => {
  const reduce = useReducedMotion();
  const hashes = Array.from({ length: 24 }, (_, i) => {
    let x = (i + 1) * 2654435761;
    let h = "0x";
    for (let j = 0; j < 12; j++) {
      x = (x * 1103515245 + 12345) >>> 0;
      h += (x >>> 24).toString(16).padStart(2, "0").slice(-1);
    }
    return h;
  });
  return (
    <Band>
      <LazyMotion features={domMax}>
        <div
          className="flex size-full items-center"
          style={{
            maskImage:
              "linear-gradient(to right, transparent, black 15%, black 85%, transparent)",
            maskSize: "100% 100%",
          }}
        >
          <m.div
            className="flex shrink-0 gap-8 font-mono text-12 whitespace-nowrap text-muted"
            animate={reduce ? undefined : { x: ["0%", "-50%"] }}
            transition={{ duration: 90, ease: "linear", repeat: Infinity }}
          >
            {[...hashes, ...hashes].map((h, i) => (
              <span key={i}>{h}</span>
            ))}
          </m.div>
        </div>
      </LazyMotion>
    </Band>
  );
};
