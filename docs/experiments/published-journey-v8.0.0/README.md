# Verified release journey for Assay 8.0.0

[Release run 45188464669, attempt 1](https://github.com/Rul1an/assay/actions/runs/45188464669/attempts/1) exercised [v8.0.0](https://github.com/Rul1an/assay/releases/tag/v8.0.0) at `081473c115ce7f03626b34f878daae2a598630c8`. Each native job completed all eleven stages of the [release-pinned documented route](https://github.com/Rul1an/assay/blob/081473c115ce7f03626b34f878daae2a598630c8/docs/guides/installed-release-journey.md), from archive acquisition to SARIF export. The CLI digest at the end matched its starting digest. For each target the connected and offline verifier reports were byte-identical, with explicit profile `privileged-mcp-action/v1` and verdict `valid`.

| Release target | Original Actions artifact ID | SHA-256 of documented-route/receipt.json |
| --- | --- | --- |
| [aarch64-apple-darwin](aarch64-apple-darwin/receipt.json) | 14826892428 | `aadd43f09de1e65a8b9bc7dfbc34543866d0037449534b30d4cb770ab75e1d7a` |
| [aarch64-unknown-linux-gnu](aarch64-unknown-linux-gnu/receipt.json) | 14826907186 | `ed51debf34512aa0430cb49a249e88846731c178c714a4c1dbd8dc9c0de02ca1` |
| [x86_64-apple-darwin](x86_64-apple-darwin/receipt.json) | 14828516356 | `ec526ad080e6e37de831de6be63cc559032efb47085a2395ccb9d314984c7b6a` |
| [x86_64-pc-windows-msvc](x86_64-pc-windows-msvc/receipt.json) | 14826722273 | `d92dacbe3ebc4500c00fd9685768d9a510216afeb71aa66703ebdade54f096c2` |
| [x86_64-unknown-linux-gnu](x86_64-unknown-linux-gnu/receipt.json) | 14824854207 | `41b2859880a842ef053e52b6f547d1ef69ea3e0957544ccf0d020ffc580e370a` |

Each target links to its original receipt. The same directory retains the connected and offline reports, `verify.json` and `verify-offline.json`. These three files come from the original artifact’s `documented-route/` directory. To check a target, hash the receipt against the table and compare the two reports byte for byte. The [proof index](../../generated/product-claim-proof.md#claim-release-install-to-verified-evidence) records the digests of the complete Actions ZIPs.

The job used a fixture upstream. On all five targets the companion server came from the verified CLI archive. This exercise covers the release archive route and the pinned guide. The Unix Filesystem quickstart, Cargo installation, editor discovery and real external effects require their own evidence. A successful stage records the expected command behavior, including the expected refusal.
