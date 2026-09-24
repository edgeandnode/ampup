import { useEffect, useState } from "react";
import {
  Button,
  ButtonGroup,
  Card,
  CodeBlock,
  Status,
  ToggleButton,
} from "@edgeandnode/eds-react";
import { ArrowSquareOutIcon } from "@edgeandnode/eds-react/icons";
import { Reveal, Section } from "@/components";
import { LINKS } from "@/data/constants";
import type { Network } from "@/lib/amp-api";

const NETWORKS: {
  id: Network;
  label: string;
  kind: string;
  ds: string;
  url: string;
}[] = [
  {
    id: "mainnet",
    label: "Ethereum",
    kind: "evm-rpc",
    ds: "eth_mainnet",
    url: "https://ethereum-rpc.example.com",
  },
  {
    id: "base",
    label: "Base",
    kind: "evm-rpc",
    ds: "base",
    url: "https://base-rpc.example.com",
  },
  {
    id: "solana-mainnet",
    label: "Solana",
    kind: "solana",
    ds: "solana_mainnet",
    url: "https://solana-rpc.example.com",
  },
];

export function Try() {
  const [net, setNet] = useState<Network>("mainnet");
  const [phase, setPhase] = useState<"idle" | "running" | "done">("idle");
  const [step, setStep] = useState(0);
  const [pct, setPct] = useState(0);
  const n = NETWORKS.find((x) => x.id === net)!;

  useEffect(() => {
    if (phase !== "running") return;
    setStep(0);
    setPct(0);
    const t0 = Date.now();
    const id = setInterval(() => {
      const s = (Date.now() - t0) / 1000;
      if (s < 0.8) setStep(0);
      else if (s < 1.6) setStep(1);
      else if (s < 5.2) {
        setStep(2);
        setPct(Math.min(100, Math.round(((s - 1.6) / 3.6) * 100)));
      } else {
        setStep(3);
        setPct(100);
        setPhase("done");
        clearInterval(id);
      }
    }, 80);
    return () => clearInterval(id);
  }, [phase]);

  const provider = [
    `kind = "${n.kind}"`,
    `network = "${net}"`,
    `url = "${n.url}"`,
    `auth_token = "\${RPC_AUTH_TOKEN}"`,
  ];
  const commands = [
    `ampctl provider register ${n.ds}_rpc ./${n.ds}.toml`,
    `ampctl manifest generate --kind ${n.kind} --network ${net} --out ${n.ds}.json`,
    `ampctl dataset register acme/${n.ds} ./${n.ds}.json --tag 1.0.0`,
    `ampctl dataset deploy acme/${n.ds}@1.0.0${
      n.kind === "evm-rpc" ? " --verify ethereum" : ""
    }`,
  ];

  return (
    <Section id="try" className="flex flex-col gap-12">
      <Reveal className="flex max-w-200 flex-col gap-4 px-4 md:px-16">
        <h2 className="text-32 font-medium text-balance md:text-40">
          From an RPC endpoint to a streaming dataset
        </h2>
        <p className="text-18 text-muted">
          Pick a network. The provider file, the manifest, and the deploy are
          generated for you. This runs against a mock; the real flow needs an
          account.
        </p>
      </Reveal>
      <div className="grid gap-8 lg:grid-cols-12 px-4 md:px-16">
        <div className="flex flex-col gap-5 lg:col-span-7">
          <ButtonGroup>
            {NETWORKS.map((x) => (
              <ToggleButton
                key={x.id}
                checked={net === x.id}
                onCheckedChange={() => {
                  setNet(x.id);
                  setPhase("idle");
                }}
                disabled={phase === "running"}
              >
                {x.label}
              </ToggleButton>
            ))}
          </ButtonGroup>
          <CodeBlock.Tabs tabs={[`${n.ds}.toml`, "deploy.sh"]}>
            <CodeBlock language="toml" secret="${RPC_AUTH_TOKEN}">
              {provider.join("\n")}
            </CodeBlock>
            <CodeBlock language="sh">{commands.join("\n")}</CodeBlock>
          </CodeBlock.Tabs>
        </div>
        <Card variant="tertiary" className="flex flex-col gap-5 lg:col-span-5">
          <ol className="flex flex-col gap-4">
            {STEPS.map((label, i) => {
              const state =
                phase === "idle"
                  ? "pending"
                  : i < step || phase === "done"
                  ? "done"
                  : i === step
                  ? "active"
                  : "pending";
              return (
                <li key={label} className="flex items-center gap-3 text-14">
                  <Status
                    variant={
                      state === "done"
                        ? "success"
                        : state === "active"
                        ? "brand"
                        : "default"
                    }
                    size="small"
                  />
                  <span className={state === "pending" ? "text-muted" : ""}>
                    {label}
                    {i === 2 && state !== "pending" ? (
                      <span className="ms-2 font-mono text-12 tabular-nums text-muted">
                        {pct}%
                      </span>
                    ) : null}
                  </span>
                </li>
              );
            })}
          </ol>
          {phase === "done" ? (
            <p className="text-14">
              Streaming. Every new block lands in the table within a second, and
              a reorg replays the corrected range.
            </p>
          ) : null}
          <ButtonGroup>
            {phase === "done" ? (
              <Button variant="tertiary" onClick={() => setPhase("idle")}>
                Start over
              </Button>
            ) : (
              <Button
                variant="secondary"
                loading={phase === "running"}
                onClick={() => setPhase("running")}
              >
                Deploy
              </Button>
            )}
          </ButtonGroup>
          <div className="flex flex-col gap-3 border-t border-subtle pt-5">
            <p className="text-14 font-medium">
              Then run it on your own chains.
            </p>
            <p className="text-14 text-muted">
              The same flow on Amp Cloud, with your endpoints and your bucket.
            </p>
            <ButtonGroup>
              <Button
                variant="primary"
                href={LINKS.contact}
                addonAfter={ArrowSquareOutIcon}
              >
                Request access to Amp Cloud
              </Button>
            </ButtonGroup>
          </div>
        </Card>
      </div>
    </Section>
  );
}

const STEPS = [
  "Provider registered",
  "Manifest registered",
  "Backfilling from genesis",
  "Following the head",
];
