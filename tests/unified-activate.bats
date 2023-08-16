#! /usr/bin/env bats
# -*- mode: bats; -*-
# ============================================================================ #
#
# Test the `flox activate' subcommand.
# We are especially interested in ensuring that the activation script works
# with most common shells, since that routine will be executed using the users
# running shell.
#
#
# ---------------------------------------------------------------------------- #

load test_support.bash;

# bats file_tags=activate


# ---------------------------------------------------------------------------- #

setup_file() {
  common_file_setup;
}


# ---------------------------------------------------------------------------- #

# Helpers for project based tests.

project_setup() {
  export PROJECT_DIR="${BATS_TEST_TMPDIR?}/project-${BATS_TEST_NUMBER?}";
  export PROJECT_NAME="${PROJECT_DIR##*/}";
  rm -rf "$PROJECT_DIR";
  mkdir -p "$PROJECT_DIR";
  pushd "$PROJECT_DIR" >/dev/null||return;
  git init;
}

project_teardown() {
  popd >/dev/null||return;
  rm -rf "${PROJECT_DIR?}";
  unset PROJECT_DIR;
  unset PROJECT_NAME;
}

activate_local_env() {
  run "$FLOX_CLI" activate -e "$PROJECT_NAME";
}


# ---------------------------------------------------------------------------- #

setup()    { common_test_setup; project_setup;       }
teardown() { project_teardown; common_test_teardown; }

# ---------------------------------------------------------------------------- #

activated_envs() {
  # Note that this variable is unset at the start of the test suite,
  # so it will only exist after activating an environment
  activated_envs=($(echo "$FLOX_PROMPT_ENVIRONMENTS"));
  echo "${activated_envs[*]}";
}

env_is_activated() {
  local is_activated;
  is_activated=0;
  for ae in $(activated_envs)
  do
    echo "activated_env = $ae, query = $1";
    if [[ "$ae" =~ "$1" ]]; then
      is_activated=1;
    fi
  done
  echo "$is_activated";
}

# ---------------------------------------------------------------------------- #

@test "'flox develop' aliases to 'flox activate'" {
  skip FIXME;
  # call `flox develop` and ensure that $TEST_ENVIRONMENT is in the list of activated envs
}


# ---------------------------------------------------------------------------- #

@test "activates environment in current dir" {
  skip FIXME;
  # call `flox activate` and ensure that $TEST_ENVIRONMENT is in the list of activated envs
}


# ---------------------------------------------------------------------------- #

@test "'flox activate' accepts explicit environment name" {
  skip FIXME;
  activate_local_env;
  assert_success;
}


# ---------------------------------------------------------------------------- #

@test "'flox activate' modifies shell prompt with 'bash'" {
  skip FIXME;
  prompt_before="${PS1@P}";
  bash -c '"$FLOX_CLI" activate -e "$PROJECT_NAME"';
  assert_success;
  prompt_after="${PS1@P}";
  assert_not_equal prompt_before prompt_after;
  assert_regex prompt_after "flox \[.*$PROJECT_NAME.*\]"
}


# ---------------------------------------------------------------------------- #

@test "'flox activate' modifies shell prompt with 'zsh'" {
  skip FIXME;
  prompt_before="${(%%)PS1}";
  zsh -c '"$FLOX_CLI" activate -e "$PROJECT_NAME"';
  assert_success;
  prompt_after="${(%%)PS1}";
  assert_not_equal prompt_before prompt_after;
  assert_regex prompt_after "\[.*$PROJECT_NAME.*\]"
}


# ---------------------------------------------------------------------------- #

@test "multiple activations are layered" {
  skip FIXME;
  # Steps
  # - Activate env1
  # - Activate env2
  # - Read activated envs with `activated_envs`
  # - Ensure that env2 (the last activated env) appears on the left
}


# ---------------------------------------------------------------------------- #

@test "activate an environment by path" {
  skip FIXME;
  # Steps
  # - Activate an environment with the -d option
  # - Ensure that the environment is activated with `env_is_activated`
  is_activated=$(env_is_activated "$PROJECT_NAME");
  assert_equal "$is_activated" "1";
}


# ---------------------------------------------------------------------------- #

@test "language specifics are set" {
  skip FIXME;
  # Steps
  # - Unset the PYTHON_PATH variable
  # - Install Python to the local environment
  # - Activate the environment
  # - Verify that PYTHON_PATH is set
}
