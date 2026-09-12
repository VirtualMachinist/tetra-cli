# Agents

Tetra is the world command. You are talking to a *world*, not to three tools.

## Do

- Author desired state in Nickel (`worlds/<vault>.ncl`, `intent/<vault>/<name>.intent.ncl`).
- Enter through `tetractl` (or `tetra --json` / later `tetra mcp`).
- Stamp work with `TETRA_SESSION` (same ULID as `FACET_SESSION`).
- Treat `facet`, `hedron`, and `kubectl` as engines. Use them when tetractl tells you to, or when debugging one projection.

## Do not

- Put tokens, kubeconfig, or vault tokens in `.ncl`.
- Add `nickel-lang-core` or `hedron-ncl` to this repo’s lockfile.
- Reimplement `facet ncl apply` / Hedron `put` / h3s admission.
- Merge Lattice, the vault, and h3s Storage into one file.
- Invent a query language. Ask Lattice, HQL, or kubectl through tetractl.
- Call `facet mcp`, `hedron hql`, and `kubectl` in a loop and call that a platform session. Start `tetractl session` first.

## First tools

```text
tetractl doctor
tetractl check worlds/prod.ncl
tetractl eval  worlds/prod.ncl --json
tetractl diff  worlds/prod.ncl --json
tetractl apply worlds/prod.ncl --dry-run --json
```
