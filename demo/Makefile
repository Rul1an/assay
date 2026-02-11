.PHONY: demo test fail validate explore init clean help

FIXTURES := demo/fixtures
CONFIG   := $(FIXTURES)/eval.yaml
SAFE     := $(FIXTURES)/traces/safe.jsonl
UNSAFE   := $(FIXTURES)/traces/unsafe.jsonl
POLICY   := $(FIXTURES)/policy.yaml

help: ## Show available commands
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

demo: fail test ## Run the full break & fix demo
	@echo ""
	@echo "  ✅ Demo complete."
	@echo "  The unsafe trace failed (exit 1), the safe trace passed (exit 0)."
	@echo "  Same policy, different traces, deterministic results."
	@echo ""
	@echo "  Next steps:"
	@echo "    make validate    Validate traces against policy"
	@echo "    make explore     Open the TUI evidence explorer"
	@echo "    assay --help     See all commands"

test: ## Run a safe trace against policy (expect PASS)
	@echo ""
	@echo "  ━━━ Safe trace (expect PASS) ━━━"
	@echo ""
	assay run --config $(CONFIG) --trace-file $(SAFE)

fail: ## Run an unsafe trace against policy (expect FAIL)
	@echo ""
	@echo "  ━━━ Unsafe trace (expect FAIL) ━━━"
	@echo ""
	-assay run --config $(CONFIG) --trace-file $(UNSAFE)

validate: ## Validate traces against policy (detailed output)
	@echo ""
	@echo "  ━━━ Validating unsafe trace ━━━"
	@echo ""
	-assay validate --config $(CONFIG) --trace-file $(UNSAFE)
	@echo ""
	@echo "  ━━━ Validating safe trace ━━━"
	@echo ""
	assay validate --config $(CONFIG) --trace-file $(SAFE)

explore: ## Open the TUI evidence explorer
	assay evidence explore $(FIXTURES)/bundle.tar.gz

init: ## Initialize a new Assay project in current directory
	assay init --hello-trace --ci github

clean: ## Remove generated artifacts
	rm -rf .assay/ bundle.tar.gz
