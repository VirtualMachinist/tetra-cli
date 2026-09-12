# tetra-owned World fixture. Imports the pack materialized by
# `facet ncl pack --out <dir>` and resolved with `--import-path <dir>`;
# never `../`. Same shape as hedron-ncl's in-tree fixture.
let platform = import "hedron-ncl/platform.ncl" in
let overlay = import "hedron-ncl/overlay/k8s-1.34-h3s-0.9.1.ncl" in
let World = import "hedron-ncl/contracts/world.ncl" in
let supported_pod = fun args =>
  overlay.restrict_pod (platform.harden_pod {
    apiVersion = "v1",
    kind = "Pod",
    metadata = { name = args.name },
    spec = {
      dnsPolicy = "Default",
      containers = [
        {
          name = "pause",
          image = "registry.k8s.io/pause:3.10",
          securityContext = {
            runAsUser = 65534,
            seccompProfile = { type = "RuntimeDefault" },
          },
        },
      ],
    },
  })
in
{
  cluster = [ supported_pod { name = "supported-pod" } ],
  intent = [
    {
      name = "tetra-g1-docs-eod",
      importance = 0.5,
      spec = {
        kind = "docs_eod",
        date = "2026-09-12",
        required_briefs = [ "foundry/tetra-cli/CHECKLIST-m1" ],
      },
    },
  ],
  calls = {
    opencollection = "1.0.0",
    info = { name = "tetra-g1-fixture", summary = "wrap parity fixture" },
    config = {
      environments = [
        { name = "lima", variables = [ { name = "FACET_PROBE", secret_ref = "kr:probe" } ] },
      ],
    },
  },
} | World
