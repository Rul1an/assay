# sendto/sendmsg with no recoverable peer (counter)

> **Status:** internal capture-fidelity detail. This page documents a counter
> that makes an address-less send visible. It adds no Runner archive member, no
> CLI output, no Trust Basis claim, and no stable report schema. The
> kernel-capture note that would surface it is produced by
> `assay runner-spike --kernel-capture` / `assay-runner-core`. Two gaps keep
> it off that path: `assay_monitor_sendto` / `assay_monitor_sendmsg` are
> compiled into the release ELF and presently unattached (`Unsupported`), and
> userspace does not read back `sendto_no_peer` / `sendmsg_no_peer` (kernel
> stat indices 14 and 15).

## What it observes

The Runner recovers a peer when a `sendto`/`sendmsg` call carries an explicit
destination address: `sendto` with a non-null sockaddr, or `sendmsg` with a
`msg_name`. Those produce a measured endpoint event.

Some sends carry no destination address in the call itself: `sendto(..., NULL,
0)` or `sendmsg` with no `msg_name`. This happens for a connected socket, where
the peer was set by an earlier `connect`. The peer is **not recoverable from that
syscall alone**.

**Socket type is not classified here.** The hook does not look up the file
descriptor's protocol, so an address-less send may be a connected datagram socket
*or* a connected stream (TCP/TLS) socket. The counter therefore counts
address-less sends generically — it does not assert they are datagram traffic.

Previously these sends were dropped silently. The kernel increments these
counters when those tracepoints run:

- eBPF increments `sendto_no_peer` / `sendmsg_no_peer` (kernel stat indices 14
  and 15) when a `sendto`/`sendmsg` has no recoverable destination address.
- The kernel-capture note gains, **only when the count is non-zero**, the suffix
  `send_no_recoverable_peer=sendto:<n> sendmsg:<m>`.

`assay_monitor_sendto` and `assay_monitor_sendmsg` are compiled into the
release ELF and inventoried `Unsupported` / unattached. Userspace does not
read back `sendto_no_peer` / `sendmsg_no_peer`. An
`assay runner-spike --kernel-capture` / `assay-runner-core` run therefore
does not surface this counter.

## What it does not do (non-claims)

- It does **not** recover the peer, and does **not** raise the network coverage
  descriptor. `datagram_peer_observed` still requires a peer that was actually
  recovered from an explicit address.
- It does **not** claim the send was datagram traffic — socket type is unknown
  at this hook.
- No payload, no byte counts, no socket identity; not a new schema field or
  archive member.

## Invariant

A run that never makes an address-less send produces a byte-identical capture
note: the suffix is appended only when the count is greater than zero. Existing
clean archives read identically before and after this change.

## Why it matters

Coverage honesty: the counter exists so an address-less send is not silently
indistinguishable from "no such send happened." An
`assay runner-spike --kernel-capture` / `assay-runner-core` run presently
has neither attach nor userspace readback, so the counter is not surfaced.
