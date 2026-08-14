# Verification + signing cost curve

Profile: `release` · payload 256 bytes/event

| events | verify ms (median) | reps | compressed bytes | gzip ratio | bytes/event | inclusion-proof hashes |
| --- | --- | --- | --- | --- | --- | --- |
| 1,000 | 16.193 | 11 | 260,847 | 0.3420 | 260.85 | 10 |
| 5,000 | 80.648 | 9 | 1,301,206 | 0.3404 | 260.24 | 13 |
| 10,000 | 160.365 | 7 | 2,601,647 | 0.3402 | 260.16 | 14 |
| 50,000 | 801.435 | 5 | 13,005,058 | 0.3393 | 260.10 | 16 |
| 100,000 | 1609.891 | 3 | 26,009,231 | 0.3392 | 260.09 | 17 |

## Linear fit (verify_ms ~ a + b·events)

- slope: 0.016092 ms/event (16.092 ms per 1k events)
- intercept: -0.5355 ms
- r²: 0.999995

## DSSE over the run anchor

- sign: 0.0294 ms (median, 101 reps)
- verify: 0.0478 ms (median, 101 reps)
