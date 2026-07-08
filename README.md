# Entl

<p align="center">
  <img src="assets/duckling.jpg" alt="Entl — a duckling in a Tyrolean hat" width="280">
  <br>
  <em><sub>Entl — Bavarian/Austrian for "little duck".</sub></em>
</p>

<p align="center">
  <a href="https://entl.dev"><img src="https://img.shields.io/badge/docs-entl.dev-111?logo=readthedocs&logoColor=white" alt="Documentation"></a>
  <a href="https://discord.gg/5G6KvdJffj"><img src="https://img.shields.io/badge/Discord-join%20the%20chat-5865F2?logo=discord&logoColor=white" alt="Join the Discord"></a>
  <a href="https://x.com/ZackMaril"><img src="https://img.shields.io/badge/X-%40ZackMaril-000?logo=x&logoColor=white" alt="Follow @ZackMaril on X"></a>
</p>

> [!WARNING]
> **Early and unstable.** Entl is pre-release and under active design — the code, the CLI, the schema, and these docs all change without notice, and much of what's described below isn't built yet. Not ready for real use. Watch along if you're curious.

Entl pulls a repository's git history and forge activity — commits, diffs, pull requests, reviews, CI, events — into data you can actually work with. A Rust engine pulls continually and streams changes as they happen, exposed through a CLI, a Rust crate, and in-process language bindings. The data lands wherever you point it: a local DuckDB by default, or Postgres, SQLite, Parquet, Kafka — **any major database, queried from any major language**. It's read-only over your repo, and stays local unless you send it somewhere.

## Docs

Full documentation — install, guides, the schema reference, and the design behind it — lives at **[entl.dev](https://entl.dev)**.

```sh
# install the CLI (Linux/macOS) — grab a release binary…
curl -fsSL https://raw.githubusercontent.com/zmaril/entl/main/install.sh | sh
# …or build it from source
cargo install --git https://github.com/zmaril/entl entl-cli

# pull a repo into a local DuckDB, then query it
entl load ./my-repo
entl query "SELECT number, title FROM gh_pull_requests WHERE state = 'OPEN'"
```

## Background & philosophy

Entl started with [Powdermonkey](https://github.com/zmaril/powdermonkey). I wanted a supervisor agent that could look at all the recent merge conflicts across a repo, reason about them, and propose refactors that would keep them from recurring. Simple enough — except merge conflicts aren't *recorded* anywhere in git. They're derived: you need two commits and a direction to know whether one would even happen, and there's a combinatorial number of possible ones, so of course git doesn't store them. I needed to walk back through history and reconstruct every conflict that had (probably) occurred, and that turned out to be real work.

The same wall shows up everywhere. I kept wanting whole-repository analyses — the [burndown charts from Hercules](https://github.com/src-d/hercules), the cityscape views of [CodeCharta](https://github.com/MaibornWolff/codecharta), churn, ownership, code age — and every time the hard part wasn't the question, it was the data wrangling: getting git and forge data into a shape you can actually query, join, and iterate over. I ran into the same problems working with git-as-data that everyone does.

Entl is my answer. Pull the history and forge activity down once, keep it current, and expose it as plain data — the raw objects plus a small algebra of operations over them (walk, diff, map, reduce, …) — from any language, into any database. It's mostly glue over [gitoxide](https://github.com/GitoxideLabs/gitoxide) and the forge APIs, and deliberately low-tech: hand-written schemas, thin bindings, no magic. I'd like to solve working-with-git-as-data once, for everyone. The thinking is written up in [notes/purpose.md](./notes/purpose.md) and the [design docs](./notes/design/).

## Feedback & contributions

Entl is early and moving fast. If something's broken, missing, or you've got a git/forge question you wish you could just query — [**file an issue**](https://github.com/zmaril/entl/issues), [**join the Discord**](https://discord.gg/5G6KvdJffj), or follow [**@ZackMaril** on X](https://x.com/ZackMaril).

To hack on entl locally, run `scripts/dev.sh` to build the Rust core and CLI; the language bindings and postgres sink have their own setup. PR titles follow [Conventional Commits](https://www.conventionalcommits.org) (`type(scope): summary`), enforced in CI.

## License

Code is MIT.

The duckling art (`assets/duckling.jpg`) was generated with Midjourney.
