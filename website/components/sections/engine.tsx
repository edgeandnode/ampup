import { useEffect, useRef, useState } from "react";
import {
  AnimatePresence,
  LazyMotion,
  domMax,
  m,
  useInView,
  useReducedMotion,
} from "motion/react";
import { Button, ButtonGroup } from "@edgeandnode/eds-react";
import {
  ArrowRightIcon,
  ArrowsClockwiseIcon,
  BracketsCurlyIcon,
  ClockCounterClockwiseIcon,
  DotsThreeOutlineIcon,
  GitBranchIcon,
  ShieldCheckIcon,
} from "@edgeandnode/eds-react/icons";
import { Dots, Reveal, Section } from "@/components";
import { LINKS } from "@/data/constants";

const STEP = 1100;
const HOLD = 1800;
const EASE = [0.25, 0.1, 0.25, 1] as const;

export function Engine() {
  const reduce = useReducedMotion();
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { amount: 0.3 });
  const [active, setActive] = useState(0);
  const [step, setStep] = useState(0);
  const [run, setRun] = useState(0);
  const playing = inView && !reduce;
  const problem = PROBLEMS[active]!;
  const last = problem.steps.length - 1;

  useEffect(() => {
    if (!playing) return;
    if (step < last) {
      const t = setTimeout(() => setStep((s) => s + 1), STEP);
      return () => clearTimeout(t);
    }
    const t = setTimeout(() => {
      setActive((a) => (a + 1) % PROBLEMS.length);
      setStep(0);
      setRun((r) => r + 1);
    }, HOLD);
    return () => clearTimeout(t);
  }, [playing, step, last]);

  const select = (i: number) => {
    setActive(i);
    setStep(0);
    setRun((r) => r + 1);
  };

  return (
    <Section id="engine" className="pb-0!" theme="dark">
      <Reveal className="flex flex-col gap-8 px-4 md:px-16">
        <h2 className="max-w-240 text-40 font-medium text-balance md:text-56">
          Chain data fails in ways a warehouse never sees.{" "}
          <span className="text-muted">
            Amp handles every one of them below the SQL.
          </span>
        </h2>
        <ButtonGroup>
          <Button href={LINKS.contact} addonAfter={ArrowRightIcon}>
            Walk through your chains with an engineer
          </Button>
        </ButtonGroup>
      </Reveal>
      <LazyMotion features={domMax}>
        <div
          ref={ref}
          className="grid lg:grid-cols-2 mt-16 border-t border-subtle"
        >
          <ul className="flex flex-col lg:px-12">
            {PROBLEMS.map(({ icon: Icon, title }, i) => {
              const open = i === active;
              return (
                <m.li
                  key={title}
                  layout="position"
                  transition={{ duration: 0.4, ease: EASE }}
                  className="border-b border-subtle last:border-b-0"
                >
                  <button
                    type="button"
                    aria-expanded={open}
                    onClick={() => select(i)}
                    className={`flex w-full items-center gap-4 py-6 text-start text-20 transition-colors ${
                      open ? "text-default" : "text-muted hover:text-default"
                    }`}
                  >
                    <Icon
                      alt=""
                      size={6}
                      className={open ? "text-brand-800" : ""}
                    />
                    <span className="font-medium">{title}</span>
                  </button>
                  {open ? (
                    <div className="grid pb-6 ps-10">
                      {PROBLEMS.map((p, j) => (
                        <m.p
                          key={p.title}
                          aria-hidden={j !== i}
                          initial={{ opacity: 0, y: 6 }}
                          animate={
                            j === i
                              ? { opacity: 1, y: 0 }
                              : { opacity: 0, y: 6 }
                          }
                          transition={{ duration: 0.4, ease: EASE }}
                          className={`col-start-1 row-start-1 max-w-140 text-16 text-muted ${
                            j === i ? "" : "invisible"
                          }`}
                        >
                          {p.body}
                        </m.p>
                      ))}
                    </div>
                  ) : null}
                  {open && playing ? (
                    <m.span
                      key={run}
                      aria-hidden
                      initial={{ scaleX: 0 }}
                      animate={{ scaleX: 1 }}
                      transition={{
                        duration: (last * STEP + HOLD) / 1000,
                        ease: "linear",
                      }}
                      className="absolute inset-x-0 -bottom-px h-px origin-left bg-brand-500"
                    />
                  ) : null}
                </m.li>
              );
            })}
          </ul>
          <div className="flex items-center justify-center overflow-clip py-12 max-lg:mt-8 max-lg:border-t lg:border-s border-subtle lg:px-12 bg-subtle">
            <div className="flex w-full max-w-120 flex-col items-center">
              <p className="w-full rounded-8 border border-subtle bg-canvas py-3 text-center font-mono text-12 tracking-[0.18em] text-muted uppercase">
                {problem.caption}
              </p>
              <span aria-hidden className="h-8 border-s border-subtle" />
              <ul className="flex min-h-80 w-full flex-col gap-2">
                <AnimatePresence initial={false} mode="popLayout">
                  {problem.steps[reduce ? last : step]!.map((r) => {
                    const t = TONE[r.tone];
                    return (
                      <m.li
                        key={r.id}
                        layout
                        initial={{ opacity: 0, y: 12 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, x: 16 }}
                        transition={{ duration: 0.35, ease: EASE }}
                        className={`flex items-center gap-3 rounded-8 border px-4 py-3 font-mono text-14 transition-colors duration-300 ${t.row}`}
                      >
                        <span
                          aria-hidden
                          className={`size-1.5 shrink-0 rounded-full transition-colors duration-300 ${t.dot}`}
                        />
                        <span className="truncate tabular-nums">{r.k}</span>
                        {r.v ? (
                          <span className="truncate text-muted tabular-nums max-sm:hidden">
                            {r.v}
                          </span>
                        ) : null}
                        <span
                          className={`ms-auto shrink-0 text-12 tracking-[0.12em] uppercase transition-colors duration-300 ${t.text}`}
                        >
                          {r.status}
                        </span>
                      </m.li>
                    );
                  })}
                </AnimatePresence>
              </ul>
            </div>
          </div>
        </div>
      </LazyMotion>
    </Section>
  );
}

type Tone = "ok" | "warn" | "bad" | "muted" | "brand";
type Row = { id: string; k: string; v?: string; status: string; tone: Tone };

const TONE: Record<Tone, { dot: string; text: string; row: string }> = {
  ok: {
    dot: "bg-status-success-default",
    text: "text-status-success-default",
    row: "border-subtle bg-canvas",
  },
  warn: {
    dot: "bg-status-warning-default",
    text: "text-status-warning-default",
    row: "border-status-warning-muted bg-status-warning-subtle",
  },
  bad: {
    dot: "bg-status-error-default",
    text: "text-status-error-default",
    row: "border-status-error-muted bg-status-error-subtle",
  },
  muted: {
    dot: "bg-foam-700",
    text: "text-muted",
    row: "border-dashed border-subtle bg-canvas",
  },
  brand: {
    dot: "bg-brand-500",
    text: "text-brand-800",
    row: "border-brand-muted bg-brand-subtlest",
  },
};

const row =
  (tone: Tone) =>
  (id: string, k: string, status: string, v?: string): Row => ({
    id,
    k,
    v,
    status,
    tone,
  });
const ok = row("ok");
const warn = row("warn");
const bad = row("bad");
const muted = row("muted");
const brand = row("brand");

const PROBLEMS = [
  {
    icon: GitBranchIcon,
    title: "Reorgs",
    body: "Every file and microbatch carries hash-linked block ranges. A fork is detected from the hashes, the orphaned range is invalidated with an explicit signal, and consumers replay the corrected range.",
    caption: "stream, mainnet blocks",
    steps: [
      [ok("a", "25,931,398", "committed", "to 401")],
      [
        ok("a", "25,931,398", "committed", "to 401"),
        ok("b", "25,931,402", "committed", "to 405"),
      ],
      [
        ok("a", "25,931,398", "committed", "to 401"),
        ok("b", "25,931,402", "committed", "to 405"),
        ok("c", "25,931,406", "committed", "to 409"),
      ],
      [
        ok("a", "25,931,398", "committed", "to 401"),
        ok("b", "25,931,402", "committed", "to 405"),
        bad("c", "25,931,406", "prev_hash mismatch", "to 409"),
      ],
      [
        ok("a", "25,931,398", "committed", "to 401"),
        ok("b", "25,931,402", "committed", "to 405"),
        brand("d", "25,931,406", "replayed", "to 408"),
      ],
      [
        ok("a", "25,931,398", "committed", "to 401"),
        ok("b", "25,931,402", "committed", "to 405"),
        ok("d", "25,931,406", "committed", "to 408"),
        ok("e", "25,931,409", "committed", "to 412"),
      ],
    ],
  },
  {
    icon: DotsThreeOutlineIcon,
    title: "Skipped slots and gaps",
    body: "Block numbers are not required to be dense. Continuity is checked on the hash chain, so a Solana slot that never existed is not a hole in your data.",
    caption: "extract, solana slots",
    steps: [
      [ok("a", "445,218,930", "hash ok")],
      [ok("a", "445,218,930", "hash ok"), ok("b", "445,218,931", "hash ok")],
      [
        ok("a", "445,218,930", "hash ok"),
        ok("b", "445,218,931", "hash ok"),
        muted("c", "445,218,932", "skipped", "no block"),
      ],
      [
        ok("a", "445,218,930", "hash ok"),
        ok("b", "445,218,931", "hash ok"),
        muted("c", "445,218,932", "skipped", "no block"),
        ok("d", "445,218,933", "hash ok", "parent 931"),
      ],
      [
        ok("a", "445,218,930", "hash ok"),
        ok("b", "445,218,931", "hash ok"),
        muted("c", "445,218,932", "skipped", "no block"),
        ok("d", "445,218,933", "hash ok", "parent 931"),
        ok("e", "445,218,934", "hash ok"),
      ],
    ],
  },
  {
    icon: ShieldCheckIcon,
    title: "Bad provider data",
    body: "With --verify, extraction recomputes Ethereum header hashes and Merkle-Patricia roots and checks Solana chain integrity. A wrong transaction from an RPC fails verification instead of landing in a table.",
    caption: "extract with verify, mainnet",
    steps: [
      [ok("a", "25,931,402", "header ok", "roots ok")],
      [
        ok("a", "25,931,402", "header ok", "roots ok"),
        ok("b", "25,931,403", "header ok", "roots ok"),
      ],
      [
        ok("a", "25,931,402", "header ok", "roots ok"),
        ok("b", "25,931,403", "header ok", "roots ok"),
        bad("c", "25,931,404", "root mismatch", "rpc 2"),
      ],
      [
        ok("a", "25,931,402", "header ok", "roots ok"),
        ok("b", "25,931,403", "header ok", "roots ok"),
        warn("c", "25,931,404", "refetching", "rpc 1"),
      ],
      [
        ok("a", "25,931,402", "header ok", "roots ok"),
        ok("b", "25,931,403", "header ok", "roots ok"),
        ok("c", "25,931,404", "header ok", "roots ok"),
      ],
      [
        ok("a", "25,931,402", "header ok", "roots ok"),
        ok("b", "25,931,403", "header ok", "roots ok"),
        ok("c", "25,931,404", "header ok", "roots ok"),
        ok("d", "25,931,405", "header ok", "roots ok"),
      ],
    ],
  },
  {
    icon: ArrowsClockwiseIcon,
    title: "Backfills that die halfway",
    body: "Backfills detect already-extracted ranges and continue from there. Streams checkpoint with cursors and resume without gaps or duplicates.",
    caption: "backfill, base from genesis",
    steps: [
      [ok("a", "0", "done", "to 12,000,000")],
      [
        ok("a", "0", "done", "to 12,000,000"),
        ok("b", "12,000,000", "done", "to 24,000,000"),
      ],
      [
        ok("a", "0", "done", "to 12,000,000"),
        ok("b", "12,000,000", "done", "to 24,000,000"),
        brand("c", "24,000,000", "extracting", "to 36,000,000"),
      ],
      [
        ok("a", "0", "done", "to 12,000,000"),
        ok("b", "12,000,000", "done", "to 24,000,000"),
        bad("c", "24,000,000", "disconnected", "at 29,417,208"),
      ],
      [
        ok("a", "0", "done", "to 12,000,000"),
        ok("b", "12,000,000", "done", "to 24,000,000"),
        warn("c", "29,417,208", "resumed", "to 36,000,000"),
      ],
      [
        ok("a", "0", "done", "to 12,000,000"),
        ok("b", "12,000,000", "done", "to 24,000,000"),
        ok("c", "24,000,000", "done", "to 36,000,000"),
        brand("d", "36,000,000", "extracting", "to head"),
      ],
    ],
  },
  {
    icon: BracketsCurlyIcon,
    title: "Decoding",
    body: "ABI, Borsh, Anchor, SCALE, and XDR decode inside the query. A new event signature is a new query, with no re-index.",
    caption: "query, decode at read time",
    steps: [
      [muted("a", "0xddf252ad1be2c89b", "raw log", "topic0")],
      [ok("a", "Transfer", "abi", "1,250.00 USDC")],
      [
        ok("a", "Transfer", "abi", "1,250.00 USDC"),
        muted("b", "0xc42079f94a635580", "raw log", "topic0"),
      ],
      [
        ok("a", "Transfer", "abi", "1,250.00 USDC"),
        ok("b", "Swap", "abi", "3.2 ETH for 9,870 USDC"),
      ],
      [
        ok("a", "Transfer", "abi", "1,250.00 USDC"),
        ok("b", "Swap", "abi", "3.2 ETH for 9,870 USDC"),
        muted("c", "AwAAAGRlcG9zaXQ", "raw data", "borsh"),
      ],
      [
        ok("a", "Transfer", "abi", "1,250.00 USDC"),
        ok("b", "Swap", "abi", "3.2 ETH for 9,870 USDC"),
        ok("c", "Deposit", "borsh", "42.5 SOL"),
      ],
    ],
  },
  {
    icon: ClockCounterClockwiseIcon,
    title: "Reproducing last quarter's number",
    body: "Datasets are immutable revisions with atomic activation. Old revisions stay for point-in-time recovery, and an as-of query over a pinned version returns the same answer every time.",
    caption: "query, as of revision 12",
    steps: [
      [ok("a", "run 1", "revision 12", "4,397,210.18")],
      [
        ok("a", "run 1", "revision 12", "4,397,210.18"),
        ok("b", "run 2", "revision 12", "4,397,210.18"),
      ],
      [
        ok("a", "run 1", "revision 12", "4,397,210.18"),
        ok("b", "run 2", "revision 12", "4,397,210.18"),
        warn("r", "revision 13", "activated", "new data"),
      ],
      [
        ok("a", "run 1", "revision 12", "4,397,210.18"),
        ok("b", "run 2", "revision 12", "4,397,210.18"),
        warn("r", "revision 13", "activated", "new data"),
        ok("c", "run 3", "revision 12", "4,397,210.18"),
      ],
      [
        ok("a", "run 1", "revision 12", "4,397,210.18"),
        ok("b", "run 2", "revision 12", "4,397,210.18"),
        warn("r", "revision 13", "activated", "new data"),
        ok("c", "run 3", "revision 12", "4,397,210.18"),
        brand("d", "run 4", "identical", "4,397,210.18"),
      ],
    ],
  },
];
