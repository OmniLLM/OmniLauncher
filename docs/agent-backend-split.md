# OmniLauncher Backend / Agent Split

> Harness conformance: OmniLauncher's backend is audited against the
> [Harness Engineering Guide](https://harness-guide.com/). See
> [`harness-conformance.md`](./harness-conformance.md) for the subsystem-by-subsystem
> checklist (agentic loop, tool system, memory & context, guardrails).


OmniLauncher is moving toward being the **backend agent** in the Omni ecosystem:

- HTTP API for the desktop shell on `:1422`
- A2A server for agent-to-agent calls on `:1423`
- plugin/tool/skill execution in the backend process
- optional self-registration into `omni-agent-hub` as an upstream

The desktop UI is extracted into the sibling repo `omni-agent-desktop`, which owns window/hotkey/screenshot behavior and talks to this backend over HTTP.

## Local topology

```text
omni-agent-desktop ── HTTP :1422 ──► OmniLauncher backend
                                      └── A2A :1423 ──► omni-agent-hub upstream registry
```

## Hub auto-registration

Set these when starting the backend:

```bash
OMNILAUNCHER_A2A_ENABLED=true \
OMNILAUNCHER_A2A_HUB_AUTO_REGISTER=true \
OMNILAUNCHER_A2A_HUB_URL=http://127.0.0.1:8222 \
OMNILAUNCHER_A2A_HUB_ADMIN_KEY=<hub-admin-key> \
OMNILAUNCHER_A2A_PUBLIC_URL=http://127.0.0.1:1423 \
make start role=backend
```

Or pass equivalent CLI overrides to `ol serve` / `omnilauncher serve`:

```bash
ol serve \
  --a2a-enabled true \
  --a2a-hub-auto-register true \
  --a2a-hub-url http://127.0.0.1:8222 \
  --a2a-hub-admin-key <hub-admin-key>
```

Fields:

| Setting | Env var | Default |
|---|---|---|
| A2A enable | `OMNILAUNCHER_A2A_ENABLED` | `false` |
| A2A public URL | `OMNILAUNCHER_A2A_PUBLIC_URL` | `http://127.0.0.1:{a2a_port}` |
| Hub admin URL | `OMNILAUNCHER_A2A_HUB_URL` | empty |
| Hub admin key | `OMNILAUNCHER_A2A_HUB_ADMIN_KEY` | empty |
| Hub upstream name | `OMNILAUNCHER_A2A_HUB_UPSTREAM_NAME` | `omnilauncher` |
| Hub prefix | `OMNILAUNCHER_A2A_HUB_PREFIX` | `@omnilauncher` |
| Auto-register | `OMNILAUNCHER_A2A_HUB_AUTO_REGISTER` | `false` |

The backend calls `POST /admin/upstreams/upsert` on the hub. This is idempotent: it creates the upstream if absent and updates the existing row when the name already exists.

## Manual verification

```bash
# Backend API
curl -s http://127.0.0.1:1422/health

# A2A card (needs A2A token from settings.json)
TOKEN=$(python3 - <<'PY'
import json, os
p=os.path.expanduser('~/.config/omnilauncher/settings.json')
print(json.load(open(p)).get('a2a_token',''))
PY
)
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:1423/.well-known/agent-card.json

# Hub sees upstream
curl -s -H "Authorization: Bearer <hub-admin-key>" http://127.0.0.1:8222/admin/upstreams
```
