# Non-Landing Docs-Only Control

This temporary branch tests the documentation-only path of the required CI
semver gate. Its pull request targets the candidate branch, not main, so the
comparison contains only this Markdown file.

The expected observation is a non-relevant semver scope, a skipped inner
checker, and a successful CI rollup. That result has not yet been measured.
It would not establish main-branch protection, review approval, or permission
to merge this temporary control.

Never merge, tag, release, or retain this document as product documentation.
