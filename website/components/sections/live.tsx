import { Reveal, Section } from "@/components";
import Live from "../Live";

export function LiveSection() {
  return (
    <Section id="live" className="flex flex-col gap-12">
      <Reveal className="flex max-w-200 flex-col gap-4 px-4 md:px-16">
        <h2 className="text-32 font-medium text-balance md:text-40">
          Under a second behind the head.
        </h2>
      </Reveal>
      <div className="px-4 md:px-16">
        <Live />
      </div>
    </Section>
  );
}
