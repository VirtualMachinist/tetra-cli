# Tetra

**The world command of the Hedronite platform.**

Nickel is the language. `tetractl` is the verb. Facet, HedronDB, and h3s stay engines.

```
tetra      — the binary
tetractl   — the kubectl-shaped alias of that same binary
```

Status: **charter** (shape locked 2026-09-12). Repo: [VirtualMachinist/tetra-cli](https://github.com/VirtualMachinist/tetra-cli).

Canon: Atrium `foundry/tetra-cli/SHAPE.md`.

## What it is

A **verbiage wrapper**. Operators and agents type `tetractl`, not `facet ncl` + `hedron` HQL + `kubectl`. Engines keep their jobs:

| You type | Engine |
|---|---|
| `tetractl check` / `eval` / `apply` | `facet ncl *` (Facet embeds `nickel-lang-core` in `hedron-ncl`) |
| intent status of names | HedronDB HQL |
| cluster get / Ready | `kubectl` on the host that has kubeconfig |
| `doctor` `session` `status` `diff` `blame` | Tetra only (gaps) |

Tetra does **not** embed `nickel-lang-core`. There is no second VM. `facet ncl` remains the engine CLI for debug.

## What embeds Nickel

**Facet** crate `hedron-ncl` (`publish = false`). Not [facet-lattice](https://crates.io/crates/facet-lattice) (Lattice run-history, 0.5.9). There is no crates.io `facet-ncl`.

## First verbs (M1, when code lands)

```text
tetractl doctor
tetractl session start
tetractl check worlds/prod.ncl
tetractl eval  worlds/prod.ncl --json
tetractl apply worlds/prod.ncl --dry-run --json
tetractl status worlds/prod.ncl
tetractl diff   worlds/prod.ncl
tetractl blame  '<Status JSON or kubectl stderr>'
```

There is nothing to `cargo run` today.

## License

Apache-2.0, unless a later NOTICE says otherwise.
