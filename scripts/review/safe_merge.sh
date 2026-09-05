#!/usr/bin/env bash
set -euo pipefail

repo="Rul1an/assay"
method=""
record_author=""
reviewer_identity=""
review_evidence_url=""
confirm_findings=false
policy_args=(--format json)

usage() {
  echo "usage: $0 PR --record-author LOGIN --reviewer-identity AGENT/INSTANCE --review-evidence-url URL --confirm-findings-disposed [--repo OWNER/REPO] (--merge|--rebase|--squash)" >&2
  exit 2
}

[[ $# -ge 1 ]] || usage
pr="$1"
shift

while (($#)); do
  case "$1" in
    --repo) repo="${2:?}"; shift 2 ;;
    --record-author) record_author="${2:?}"; shift 2 ;;
    --reviewer-identity) reviewer_identity="${2:?}"; shift 2 ;;
    --review-evidence-url) review_evidence_url="${2:?}"; shift 2 ;;
    --unprotected-require-check) policy_args+=("$1" "${2:?}"); shift 2 ;;
    --confirm-findings-disposed) confirm_findings=true; shift ;;
    --merge|--rebase|--squash) method="$1"; shift ;;
    *) usage ;;
  esac
done

[[ -n "$record_author" && -n "$reviewer_identity" && -n "$review_evidence_url" ]] || usage
[[ -n "$method" ]] || usage
[[ "$confirm_findings" == true ]] || usage

readiness="$(dirname "$0")/pr_landing_readiness.py"
report="$($readiness "$pr" --repo "$repo" "${policy_args[@]}")"

if [[ "$(jq -r '.landing_candidate' <<<"$report")" != "true" ]]; then
  jq -r '.blockers[] | "BLOCKED: " + .' <<<"$report" >&2
  exit 1
fi

if ! jq -e --arg record_author "$record_author" '
  any(.review_candidates[];
      .record_author == $record_author and .verdict == "READY" and .current_head == true)
' <<<"$report" >/dev/null; then
  echo "BLOCKED: no current-head READY record by declared record author '$record_author'" >&2
  exit 1
fi

head="$(jq -r '.pr.headRefOid' <<<"$report")"
python3 "$(dirname "$0")/verify_review_identity.py" \
  --repo "$repo" --pr "$pr" --head "$head" \
  --record-author "$record_author" --reviewer-identity "$reviewer_identity" \
  --review-evidence-url "$review_evidence_url" \
  --pr-author "$(jq -er '.pr.author.login' <<<"$report")"
printf 'Landing PR #%s at exact head %s\n' "$pr" "$head"
printf 'Check policy: %s\n' "$(jq -r '.check_policy' <<<"$report")"
printf 'Operator confirms actionable findings are disposed; reviewer declaration is linked above.\n'
gh pr merge "$pr" --repo "$repo" "$method" --match-head-commit "$head"
