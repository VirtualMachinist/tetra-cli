# Agents

Tetra is the platform CLI. You are talking to a *world*, not to three tools.

## Do

- Author desired state in Nickel (`world.ncl`). Check it with `tetra check`.
- Enter through `tetra mcp` or `tetra --json` when you need more than one projection.
- Stamp work with `TETRA_SESSION`. Do not invent a second join key.
- Treat `facet`, `hedron`, and `kubectl` as engines. Use them when tetra tells you to, or when you are debugging one projection.

## Do not

- Put tokens, kubeconfig secrets, or vault tokens in `.ncl`.
- Merge Lattice, the vault, and h3s Storage into one file because it is convenient.
- Invent a query language. Ask Lattice, HQL, or kubectl.
- Call `facet mcp`, `hedron hql`, and `kubectl` in a loop and call that a platform session. That is the old wrapper. Start a tetra session first.

## First tools

```text
tetra doctor
tetra check world.ncl
tetra eval  world.ncl --json
tetra diff  world.ncl --json
tetra apply world.ncl --dry-run --json
```
