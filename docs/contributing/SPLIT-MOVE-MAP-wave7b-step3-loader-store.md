# Wave7B Step3 move-map: closure

Boundary legend:
- `loader.rs`: public API/types + delegation only.
- `loader_internal/tests.rs`: all loader unit tests and test env guard.
- `loader_internal/{run,resolve,parse,digest,compat}.rs`: unchanged functional boundaries from Step2.
- `store_internal/{schema,results,episodes}.rs`: unchanged helper boundaries from Step2.

Step3 moves:
- `loader.rs` test module (`mod tests`) -> `loader_internal/tests.rs`.
- `loader.rs` test-only wrappers removed:
  - `is_valid_pack_name`
  - `version_satisfies`
  - `levenshtein_distance`
- Equivalent tests now call internal impl boundaries directly:
  - `compat::version_satisfies_impl`
  - `resolve::is_valid_pack_name_impl`
  - `resolve::levenshtein_distance_impl`

Closure guarantees:
- Loader facade has no test code and no private logic functions.
- Anchor tests are executed by fully-qualified names under `loader_internal::tests`.
- Store helper boundaries remain as in Step2 (no broad scope expansion).
