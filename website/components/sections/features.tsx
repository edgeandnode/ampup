import { CodeBlock, Status } from "@edgeandnode/eds-react";
import {
  ArrowsClockwiseIcon,
  FingerprintIcon,
  PlugsConnectedIcon,
  PulseIcon,
  RobotIcon,
} from "@edgeandnode/eds-react/icons";
import { Section } from "@/components";
import { useHeads } from "@/lib/use-heads";

export function Features() {
  const mainnet = useHeads().find((h) => h.network === "mainnet")!;

  return (
    <Section id="features-alt" theme="dark" className="py-0!">
      <ul className="grid border-b border-subtle sm:grid-cols-2 lg:grid-cols-5">
        {FACTS.map(({ icon: Icon, title, body }) => (
          <li
            key={title}
            className="flex min-h-44 flex-col justify-between gap-10 border-subtle p-6 max-lg:border-b lg:border-e lg:last:border-e-0"
          >
            <Icon alt="" size={5} className="text-muted" />
            <span className="flex flex-col gap-1.5 text-14">
              <span className="font-medium">{title}</span>
              <span className="text-muted">{body}</span>
            </span>
          </li>
        ))}
      </ul>
      <h2 className="max-w-160 px-4 py-16 text-32 font-medium text-balance md:px-16 md:py-24 md:text-40">
        Every chain as tables.{" "}
        <span className="text-muted">Ready for the tools you run.</span>
      </h2>

      {STEPS.map((step, i) => (
        <div
          key={step.title}
          className="grid border-t border-subtle lg:grid-cols-2"
        >
          <div className="flex flex-col gap-4 px-4 py-12 md:px-16 md:py-16">
            <span className="font-mono text-12 text-muted">0{i + 1}</span>
            <h3 className="text-24 font-medium">{step.title}</h3>
            <p className="max-w-100 text-16 text-muted">{step.body}</p>
          </div>
          <div className="flex flex-col justify-center gap-4 bg-[repeating-linear-gradient(to_right,transparent_0_7px,var(--border-color-subtle)_7px_8px)] p-4 max-lg:border-t md:p-10">
            <CodeBlock language={step.lang} label={step.file}>
              {step.code}
            </CodeBlock>
            <CodeBlock language="text">{step.output}</CodeBlock>
            {i === 0 ? (
              <p className="flex items-center gap-2 font-mono text-12 text-muted tabular-nums">
                <Status variant="success" size="small" />
                genesis to {mainnet.ampHead.toLocaleString("en-US")},{" "}
                {mainnet.lagMs} ms behind the head
              </p>
            ) : null}
          </div>
        </div>
      ))}
    </Section>
  );
}

const FACTS = [
  {
    icon: PulseIcon,
    title: "It follows the head.",
    body: "Under a second behind the chain, on every network you name.",
  },
  {
    icon: PlugsConnectedIcon,
    title: "Your tools keep working.",
    body: "Flight SQL, ADBC, and Parquet that any engine reads.",
  },
  {
    icon: ArrowsClockwiseIcon,
    title: "Reorgs replay themselves.",
    body: "Hash-linked ranges drop orphaned blocks and resend the right ones.",
  },
  {
    icon: FingerprintIcon,
    title: "Every answer repeats.",
    body: "A pinned revision returns the same result every time.",
  },
  {
    icon: RobotIcon,
    title: "Agents use the same CLI.",
    body: "Skills for ampctl and ampsql ship with Amp.",
  },
];

const QUERY = `SELECT
  CAST(date_trunc('day', timestamp) AS DATE) AS day,
  count(*) AS transfers
FROM "acme/eth_mainnet".logs
WHERE topic0 = evm_topic('Transfer(address indexed from, address indexed to, uint256 value)')
GROUP BY 1
ORDER BY 1 DESC
LIMIT 5`;

const STEPS: {
  title: string;
  body: string;
  file: string;
  lang: "toml" | "sql" | "py";
  code: string;
  outputLabel: string;
  output: string;
}[] = [
  {
    title: "Extract",
    body: "A provider is a TOML file with a kind, a network, and a URL. Amp generates the tables from it, syncs genesis to head, and checks every header hash on the way in.",
    file: "mainnet.toml",
    lang: "toml",
    code: `kind = "evm-rpc"
network = "mainnet"
url = "https://ethereum-rpc.example.com"`,
    outputLabel: '"acme/eth_mainnet".blocks',
    output: `+------------------+-------------------------------+
| column_name      | data_type                     |
+------------------+-------------------------------+
| block_num        | UInt64                        |
| timestamp        | Timestamp(Nanosecond, +00:00) |
| hash             | FixedSizeBinary(32)           |
| parent_hash      | FixedSizeBinary(32)           |
| miner            | FixedSizeBinary(20)           |
| gas_used         | UInt64                        |
| base_fee_per_gas | Decimal128(38, 0)             |
+------------------+-------------------------------+
19 more columns, plus transactions and logs`,
  },
  {
    title: "Query and stream",
    body: "Standard SQL over Arrow Flight. Events decode inside the query, so a new event signature needs no re-index. Add SETTINGS stream = true and the same statement streams in block order.",
    file: "transfers.sql",
    lang: "sql",
    code: QUERY,
    outputLabel: "ampsql",
    output: `+------------+-----------+
| day        | transfers |
+------------+-----------+
| 2026-09-22 | 1184302   |
| 2026-09-21 | 1209877   |
| 2026-09-20 | 1151640   |
| 2026-09-19 | 1098215   |
| 2026-09-18 | 1236004   |
+------------+-----------+`,
  },
  {
    title: "Deliver",
    body: "The same result lands in Polars as zero-copy Arrow, or in a Snowflake or Postgres table through an ADBC export job, exactly once.",
    file: "notebook.py",
    lang: "py",
    code: `df = client.sql(q).to_polars()

# or keep a warehouse table in sync
client.sql(q).load(connection=sf, destination="raw.transfers_daily")`,
    outputLabel: "df",
    output: `shape: (5, 2)
┌────────────┬───────────┐
│ day        ┆ transfers │
│ ---        ┆ ---       │
│ date       ┆ u64       │
╞════════════╪═══════════╡
│ 2026-09-22 ┆ 1184302   │
│ 2026-09-21 ┆ 1209877   │
│ 2026-09-20 ┆ 1151640   │
│ 2026-09-19 ┆ 1098215   │
│ 2026-09-18 ┆ 1236004   │
└────────────┴───────────┘`,
  },
];
