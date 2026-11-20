# OpenTimestamps proofs

Bitcoin-blockchain timestamps proving when each release commit existed.

| File | Proves |
|------|--------|
| `v0.1.0.commit` | full SHA of the `v0.1.0` release commit (`93943a3`) |
| `v0.1.0.commit.ots` | OpenTimestamps proof for that file |

## Verify

```sh
ots verify timestamps/v0.1.0.commit
```

A freshly created proof is *pending*: it carries calendar-server attestations,
and the Bitcoin attestation is added a few hours later once the transaction is
mined. To upgrade a pending proof with the confirmed Bitcoin attestation:

```sh
ots upgrade timestamps/v0.1.0.commit.ots
```

The SHA stored in `v0.1.0.commit` matches `git rev-parse v0.1.0^{commit}`, so the
proof binds the timestamp to the exact released source tree.
