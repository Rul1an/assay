# Wave55 Step1 Checklist - Evidence Schema Facade Split

- [ ] Step1 is based on `origin/main`
- [ ] `schema.rs` remains the public `assay evidence schema` command facade
- [ ] Schema registry descriptors live in `schema/registry.rs`
- [ ] Report/metadata/error DTOs live in `schema/reports.rs`
- [ ] JSON/JSONL validation lives in `schema/validate.rs`
- [ ] Text/JSON rendering lives in `schema/write.rs`
- [ ] No receipt schema JSON files are changed
- [ ] No importer command files are changed
- [ ] No docs/reference schema contract files are changed
- [ ] No `.github/workflows/**` files are changed
- [ ] `bash scripts/ci/review-wave55-evidence-contract-schema-step1.sh` passes
