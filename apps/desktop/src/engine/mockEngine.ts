import type { StageEngine, StageEvent } from "./StageEngine";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const mockEngine: StageEngine = {
  async *send(_provider: string, _text: string): AsyncIterable<StageEvent> {
    const intro = "I'll start by reading the parser. ";
    for (const word of intro.split(" ")) {
      yield { kind: "token", text: word + " " };
      await sleep(40);
    }
    yield { kind: "tool", id: "t1", name: "read_file", status: "running", detail: "parser.rs" };
    await sleep(300);
    yield { kind: "tool", id: "t1", name: "read_file", status: "done", detail: "parser.rs" };

    const outro = "Done — the parser is split into a lexer and a grammar module.";
    for (const word of outro.split(" ")) {
      yield { kind: "token", text: word + " " };
      await sleep(40);
    }
    yield { kind: "done" };
  },
};
