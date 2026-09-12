# Tetra

**The CLI language of the Hedronite platform.**

One Nickel world. Three engines. One verb surface.

```
tetra      — the binary
tetractl   — the kubectl-shaped alias
```

Status: **charter** · Language: Nickel · Engines: Facet · HedronDB · h3s
Repo: [VirtualMachinist/tetra-cli](https://github.com/VirtualMachinist/tetra-cli)

Built by [Hedronite](https://hedronite.com)'s [VirtualMachinist](https://github.com/VirtualMachinist).

---

Facet, HedronDB, and Hedronetes are one platform. Their *product* borders dissolve
into a single declared world. Their *engines* stay split: Lattice still records
calls, the vault still records intent, h3s still runs workloads.

Nickel is the declaration language. `tetra` is the verb language.

```text
You write    .ncl                 (contracts, merge, overlays)
tetra eval   the world            (typed, merged, documented)
tetra apply  slices of it         (to Facet, HedronDB, h3s)
tetra diff   desired vs observed
tetra recon  until they meet
```

## Docs

| Doc | What it is |
|---|---|
| [docs/PLATFORM.md](docs/PLATFORM.md) | Doctrine. What unifies, what must not. |
| [docs/TETRACTL.md](docs/TETRACTL.md) | Verb surface. Commands, envelopes, MCP. |
| [docs/COMPANION_ADDENDUM.md](docs/COMPANION_ADDENDUM.md) | What changes in `COMPANION_SPEC.md`. |
| [contracts/platform.ncl](contracts/platform.ncl) | The Tetra contract the world is checked against. |
| [examples/world.ncl](examples/world.ncl) | A small merged world. |

## Status

Charter tree. The fleet's Nickel glue lands `eval` next.
This repo is the contract and the verb surface, not a working binary yet.

When code lands it will be Rust, embed `nickel-lang-core`, speak `--json`,
and expose `tetra mcp` over stdio. There is nothing to `cargo run` today.

## License

Apache-2.0, unless a later NOTICE says otherwise.
