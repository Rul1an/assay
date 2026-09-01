# shellcheck shell=bash
# Sourced, not executed: no shebang, so the target shell is declared instead.
# The canonical set of git environment variables that must not leak into a question about a tree.
#
# One list, sourced by every test script that drives git against a scratch repository. It existed
# three times before -- twice hand-written in shell and once in Python -- and the copies had already
# drifted: the shell ones were each missing a different subset. `test-tracked-paths-parity.sh`
# asserts this list equals the Python `GIT_ENV` tuple, so the remaining two cannot drift apart.
#
# GIT_CONFIG_COUNT is the subtle one: without it git ignores the numbered GIT_CONFIG_KEY_n and
# GIT_CONFIG_VALUE_n pairs entirely, so dropping the count drops the injected configuration too.
GIT_ENV_VARS="GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_OBJECT_DIRECTORY GIT_COMMON_DIR \
GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT GIT_CONFIG_GLOBAL \
GIT_CONFIG_SYSTEM GIT_CEILING_DIRECTORIES"

sgit() {
  local unsets=()
  local var
  for var in ${GIT_ENV_VARS}; do
    unsets+=(-u "${var}")
  done
  env "${unsets[@]}" git "$@"
}
