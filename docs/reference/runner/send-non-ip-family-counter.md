# sendto/sendmsg to a non-IP family (counter)

> **Status:** internal capture-fidelity detail. This page documents a counter
> that keeps the datagram peer label honest. It adds no Runner archive member,
> no CLI output, no Trust Basis claim, and no stable report schema. It is
> additive and visible only when the relevant syscalls are used.

## What it observes

The Runner normalizes only IPv4 (`AF_INET`) and IPv6 (`AF_INET6`) destination
addresses into endpoint evidence. A `sendto`/`sendmsg` to another socket family —
for example an `AF_UNIX` send — carries an address the Runner does not turn into
an IP endpoint. Those sends were previously skipped silently at the family
filter.

**Socket type is not classified here.** The hook does not look up the file
descriptor's protocol, so a non-IP send is not asserted to be datagram traffic;
the counter records non-IP-family `sendto`/`sendmsg` generically.

This counter records them so the `datagram_peer_observed` coverage label stays
honest: that label reflects **IP peers only**, and a non-IP send is neither an IP
peer nor a silently lost observation.

- eBPF increments `sendto_non_ip_family` / `sendmsg_non_ip_family` (kernel stat
  indices 16 and 17) when a `sendto`/`sendmsg` is skipped because its socket
  family is not `AF_INET`/`AF_INET6`. The `connect` path passes a disabled
  sentinel and is unaffected.
- The kernel-capture note gains, **only when the count is non-zero**, the suffix
  `send_non_ip_family=sendto:<n> sendmsg:<m>`.

## What it does not do (non-claims)

- It does **not** recover or classify the non-IP peer, and does **not** raise the
  network coverage descriptor.
- It does **not** capture the socket *type* (`SOCK_DGRAM` vs `SOCK_STREAM`); that
  needs an fd-to-socket lookup and is out of scope here.
- It is not a new schema field or archive member, and it does not change the
  `connect` path.

## Invariant

A run with no non-IP `sendto`/`sendmsg` produces a byte-identical capture note:
the suffix is appended only when the count is greater than zero. Existing clean
archives read identically before and after this change.

## Why it matters

Coverage honesty: an operator can tell that sends to non-IP families occurred, so
the IP-only `datagram_peer_observed` label is read in context rather than mistaken
for the complete picture.
