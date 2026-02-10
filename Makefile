.PHONY: demo test fail explore init clean help

FIXTURES := demo/fixtures
CONFIG   := $(FIXTURES)/eval.yaml
SAFE     := $(FIXTURES)/traces/safe.jsonl
UNSAFE   := $(FIXTURES)/traces/unsafe.jsonl
POLICY   := $(FIXTURES)/policy.yaml

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

demo: fail test ## Run the full break & fix demo
	@echo ""
	@echo "  ✅ Demo complete. The unsafe trace failed, the safe trace passed."
	@echo "  That's Assay: deterministic policy enforcement for AI agents."

test: ## Run a safe trace against policy (should PASS)
	@echo "━━━ Safe trace (expect PASS) ━━━"
	@ls -d demo/fixtures > /dev/null 2>&1 || (echo "Error: demo/fixtures not found" && exit 1)
	cd $(CURDIR)/demo/fixtures && assay run --config eval.yaml --trace-file traces/safe.jsonl

fail: ## Run an unsafe trace against policy (should FAIL)
	@echo "━━━ Unsafe trace (expect FAIL) ━━━"
	@ls -d demo/fixtures > /dev/null 2>&1 || (echo "Error: demo/fixtures not found" && exit 1)
	-cd $(CURDIR)/demo/fixtures && assay run --config eval.yaml --trace-file traces/unsafe.jsonl

explore: ## Open the TUI evidence explorer
	cd $(CURDIR)/demo/fixtures && assay evidence explore --bundle bundle.tar.gz

validate: ## Validate traces against policy
	cd $(CURDIR)/demo/fixtures && assay validate --config eval.yaml --trace-file traces/unsafe.jsonl
	cd $(CURDIR)/demo/fixtures && assay validate --config eval.yaml --trace-file traces/safe.jsonl

init: ## Initialize a new Assay project in current directory
	assay init --hello-trace --ci github

clean: ## Remove generated artifacts
	rm -rf .assay/ bundle.tar.gz
