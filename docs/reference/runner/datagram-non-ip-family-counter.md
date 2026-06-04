# Datagram sends to a non-IP family (counter)

> **Status:** internal capture-fidelity detail. This page documents a counter
> that keeps the datagram peer label honest. It adds no Runner archive member,
> no CLI output, no Trust Basis claim, and no stable report schema. It is
> additive and visible only when the relevant syscalls are used.

## What it observes

The Runner normalizes only IPv4 (`AF_INET`) and IPv6 (`AF_INET6`) destination
addresses into endpoint evidence. A `sendto`/`sendmsg` to another socket family —
for example `AF_UNIX` datagram sends — carries an address the Runner does not turn
into an IP endpoint. Those sends were previously skipped silently at the family
filter.

This counter records them so the `datagram_peer_observed` coverage label stays
honest: that label reflects **IP peers only**, and a non-IP datagram send is
neither an IP peer nor a silently lost observation.

- eBPF increments `sendto_non_ip_family` / `sendmsg_non_ip_family` (kernel stat
  indices 16 and 17) when a datagram send is skipped because its socket family is
  not `AF_INET`/`AF_INET6`. The `connect` path passes a disabled sentinel and is
  unaffected.
- The kernel-capture note gains, **only when the count is non-zero**, the suffix
  `datagram_non_ip_family=sendto:<n> sendmsg:<m>`.

## What it does not do (non-claims)

- It does **not** recover or classify the non-IP peer. A counted send contributes
  no endpoint and does **not** raise the network coverage descriptor.
- It does not capture the socket *type* (`SOCK_DGRAM` vs `SOCK_STREAM`); that
  needs an fd-to-socket lookup and is out of scope here.
- It is not a new schema field or archive member, and it does not change the
  `connect` path.

## Invariant

A run with no non-IP datagram sends produces a byte-identical capture note: the
suffix is appended only when the count is greater than zero. Existing clean
archives read identically before and after this change.

## Why it matters

Coverage honesty: an operator can tell that datagram sends to non-IP families
occurred, so the IP-only `datagram_peer_observed` label is read in context rather
than mistaken for the complete datagram picture.
