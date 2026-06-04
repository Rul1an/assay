# Datagram sends with no recoverable peer (counter)

> **Status:** internal capture-fidelity detail. This page documents a counter
> that makes an already-declared network blind spot visible. It adds no Runner
> archive member, no CLI output, no Trust Basis claim, and no stable report
> schema. It is additive and visible only when the relevant syscalls are used.

## What it observes

The Runner recovers a datagram peer when a `sendto`/`sendmsg` call carries an
explicit destination address: `sendto` with a non-null sockaddr, or `sendmsg`
with a `msg_name`. Those produce a measured `sendto`/`sendmsg` endpoint event and
can raise the network coverage descriptor to `datagram_peer_observed`.

A connected datagram socket is different: the peer was set by an earlier
`connect`, so the individual `sendto(..., NULL, 0)` / `sendmsg` with no
`msg_name` carries no address in that call. The peer is **not recoverable from
that syscall alone**. The network coverage descriptor already declares this as a
blind spot ("connected datagram sends without an explicit sockaddr require
connect evidence to recover the peer").

Previously those calls were dropped silently. This counter records them so the
blind spot is visibly *exercised* rather than invisible:

- eBPF increments `sendto_no_peer` / `sendmsg_no_peer` (kernel stat indices 14
  and 15) when a datagram send has no recoverable destination address.
- The kernel-capture note gains, **only when the count is non-zero**, the
  suffix `datagram_no_recoverable_peer=sendto:<n> sendmsg:<m>`.

## What it does not do (non-claims)

- It does **not** recover the peer. A counted send contributes no endpoint and
  does **not** raise the network coverage descriptor. `datagram_peer_observed`
  still requires a peer that was actually recovered from an explicit address.
- No payload, no byte counts, no socket identity — only that an
  address-less datagram send was observed.
- It is not a new schema field or archive member.

## Invariant

A run that never makes an address-less datagram send produces a byte-identical
capture note: the suffix is appended only when the count is greater than zero.
Existing clean archives read identically before and after this change.

## Why it matters

Coverage honesty: an operator can now tell the difference between "no datagram
sends happened" and "datagram sends happened but their peers were not
recoverable from the send call." The latter is exactly the case where a
connect-correlation step (future, separate work) would be needed to attribute
the peer — and where, until then, an absence-of-network claim must stay
conservative.
