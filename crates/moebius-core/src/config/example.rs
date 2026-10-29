//! The commented example of `moebius.toml` that `moebius init` writes, every key in it at its default. The reference
//! test reads its keys to check that the configuration reference names each.

/// The commented example `moebius init` writes. Every value in it is the default.
pub const EXAMPLE: &str = r#"# Moebius server configuration. Every key is optional; the values below are the defaults.

# Where Moebius keeps its store, blobs, clones and worktrees.
data_dir = ".moebius"

# The address the desktop app connects to. Keep it on a private network (ADR 0003).
listen = "127.0.0.1:7171"

# The file holding the token the desktop app authenticates with; relative paths are inside data_dir.
token_file = "token"

# How many runs may work at the same time, across all boards.
run_limit = 3

# When the daily Retro runs, as HH:MM in the server's local time.
retro_at = "23:30"

# Rules every run must follow, as Markdown with one section per rule (a heading `## name` and its text); relative paths
# are inside this file's directory, and the desktop app edits the file. A repository adds its own in .moebius/rules.md
# but cannot replace these. While the file does not exist, only the built-in rules apply. Not allowed together with
# [harness], whose repository holds rules.md.
# rules_file = "rules.md"

# The daily stretch in which trials, replays and autoresearch may run without asking.
[trial_window]
start = "00:00"
end = "06:00"

# Each proposal of the Retro is also tried on a regression set of each repository its cards belong to: the latest `size`
# merged cards, those with a reproduction test (.moebius/reproduce) first. It is positive only when none of them fails
# where it passed before. This adds at most `size` trial runs per repository to a proposal. With gate = false the
# regression set is not tried.
[regression]
size = 5
gate = true

# A code run is told which files are likely relevant to its issue, as a hint and not a limit: the decision point
# `localize` ranks at most `files` (1 to 10) of the repository's files, first by Moebius's own search over them. With
# `enabled` false no run is told. A repository can turn it on or off for itself, and name its own number of files, in
# the file .moebius/localize on its default branch.
[localize]
enabled = true
files = 10

# How often the board engine runs the column scripts, and how many coding rounds a card gets.
[board]
tick_ms = 2000
round_limit = 5

# The most tokens (input plus output) all runs together may use in a day of the server's local time. Once it is spent,
# the router refuses model requests and no card starts or continues work until the next day. Without it there is no limit.
# [budgets]
# day_tokens = 20000000

# A repository without a forge names its own checks in .moebius/checks on its default branch: a command run in a
# worktree of the change before it is reviewed. They are stopped after timeout_seconds (at least 1) and then fail.
[checks]
timeout_seconds = 1800

# The router: the only model provider the harnesses see (ADR 0006). It listens on localhost only.
[router]
listen = "127.0.0.1:7172"

# Where the router sends requests, one upstream per protocol (openai or anthropic). A key is never in this file: it is
# read from the environment variable named by api_key_env, or from the secret named by api_key_secret (a key typed in
# the desktop app is stored under a name in <data_dir>/secrets.toml). Set one of them, or neither for a local server.
# [[router.upstreams]]
# protocol = "openai"
# base_url = "http://127.0.0.1:8080"      # a local server such as Radiance
# [[router.upstreams]]
# protocol = "anthropic"
# base_url = "https://api.anthropic.com"
# api_key_env = "ANTHROPIC_API_KEY"       # or: api_key_secret = "anthropic"

# An upstream may log in with OAuth instead of a key, from the Models page of the desktop app. The flow is "device" (a
# code to enter at the provider) or "code" (the provider's page in the browser, sent back to 127.0.0.1). The client id
# and URLs are the provider's; the refresh token is kept in secrets.toml under the name `secret`. Using a subscription
# through a third-party tool may break the provider's terms: the app asks you to confirm that before a login starts.
# [[router.upstreams]]
# protocol = "anthropic"
# base_url = "https://api.example.com"
# [router.upstreams.oauth]
# flow = "device"                         # or code
# client_id = "the-client-id"
# token_url = "https://login.example.com/oauth/token"
# device_url = "https://login.example.com/oauth/device/code"   # for device
# authorize_url = "https://login.example.com/oauth/authorize"  # for code
# scopes = ["models"]
# redirect_port = 54545                   # for code; any free port when not set
# redirect_path = "/callback"             # for code
# secret = "example-login"

# A model class names an ordered list of models; an agent, the teacher or a chat may name the class where it names a
# model. A request for a class goes to the first of its models that has tokens left today, on the first upstream of the
# request's protocol; when an upstream fails before it answers (it cannot be reached, is not logged in, or answers 5xx,
# 429 or 404) the next upstream is tried, then the next model. A model is a name, or a table with day_tokens: the most
# tokens it may use in a day.
# [router.classes]
# fast = ["qwen3.8-flash-next", "claude-haiku-5"]
# strong = [{ model = "claude-opus-5", day_tokens = 5000000 }, "claude-sonnet-5-5"]

[pi_bolt]
program = "pi-bolt"

# Claude Code's command line, for the agents whose harness is "claude-code". It has no bridge: the router checks the
# tool calls in its model's answers instead (see docs/reference/router.md).
[claude_code]
program = "claude"

# The teacher model checks the rules written in prose (a rule the built-in patterns cannot see) on every tool call.
# It is asked through the router, so its tokens are counted against the run being checked, and it needs an upstream for
# its protocol. Without a teacher, a call is refused whenever any prose rule applies (fail closed); without prose
# rules the teacher is never asked. The answer must come within timeout_seconds (1 to 29: the plugin host cuts a
# decider off after 30 seconds); a teacher that fails, is too slow or answers nonsense refuses the call.
# [teacher]
# model = "qwen3.8-flash-next"
# protocol = "openai"                     # or anthropic
# timeout_seconds = 20

# The decision model decides alongside the teacher for the kinds of decision it is trained on (ADR 0010). It is asked
# about every judgement held back from training (see docs/reference/decision-model.md) in the background, and how
# often it chooses what was recorded is measured. Once it has been asked about min_judgements judgements of a kind and
# agrees on at least the share `agreement`, a proposal in the inbox offers it that kind; merging the proposal makes the
# model decide it. `url` is an OpenAI-compatible base URL, called at <url>/chat/completions. The key is read from the
# environment variable named by api_key_env or the secret named by api_key_secret, never from this file. Without this
# section nothing of this happens.
# [decision_model]
# url = "http://127.0.0.1:8081/v1"
# model = "moebius-decider"
# api_key_env = "DECISION_MODEL_KEY"      # optional; or: api_key_secret = "decision-model"
# timeout_seconds = 20                    # 1 to 29, like the teacher's
# shadow = ["tool_call", "merge"]         # the kinds to shadow; all of them when not given
# agreement = 0.95                        # the share of equal choices needed, above 0 and at most 1
# min_judgements = 100                    # how many held-out judgements of a kind must have been asked about

# A plugin in its own process: a program that speaks JSON Lines on its standard input and output (see
# docs/reference/plugin-api.md). It is started with args in this file's directory when the server starts, and started
# again after a pause when it stops; its standard error goes to data_dir/plugins/<name>.log. Its manifest must say the
# same name. It answers the kinds of decision in `decides`; a kind has one decider, so naming tool_call replaces the
# guard. A path with a directory in it is relative to this file's directory; a bare name is looked up on the PATH.
# [[plugins]]
# name = "allow-reads"
# path = "plugins/allow-reads"
# args = ["--plugin"]
# decides = ["start_card"]

# A machine that runs harnesses over SSH. An agent whose `runner` names it, or a column script that passes its name
# to moebius.start_run, runs there: the run's branch is pushed to a clone under working_dir, the harness is started
# over SSH, it reaches the router through a reverse tunnel, and its commits are fetched back when it ends.
# [[runners]]
# name = "gpu-box"
# host = "gpu.example.com"
# user = "moebius"                        # without it, the SSH configuration's
# port = 22                               # without it, the SSH configuration's
# key_file = "gpu-box.key"                # without it, the SSH configuration's; relative to this file's directory
# working_dir = "/srv/moebius"            # absolute, on the machine
# max_runs = 1                            # how many runs may go on the machine at once
# bin_dir = "/usr/local/bin"              # where pi-bolt and claude are on the machine; without it, its PATH
# ssh = "ssh"                             # the ssh program

# Moebius acts as its own GitHub App. The key file is read at start; relative paths are inside this file's directory.
# [github]
# app_id = 123456
# private_key_file = "github-app.pem"
# api_url = "https://api.github.com"
# poll_seconds = 30                       # how often each GitHub repository's issues are polled

# Repositories on GitLab, gitlab.com or one hosted elsewhere, with a personal, project or group access token (scope
# api). The token is read at start from token_file, or from the secret named by token_secret, never from this file.
# [gitlab]
# url = "https://gitlab.com"
# token_file = "gitlab-token"
# token_secret = "gitlab"                 # instead of token_file
# poll_seconds = 30                       # how often each GitLab repository's issues and merge requests are polled

# Repositories on Forgejo or Gitea, Codeberg or one hosted elsewhere, with an access token (scopes write:issue and
# write:repository). It is read at start from token_file, or from the secret named by token_secret.
# [forgejo]
# url = "https://codeberg.org"
# token_file = "forgejo-token"
# token_secret = "forgejo"                # instead of token_file
# poll_seconds = 30                       # how often each repository's issues and pull requests are polled

# The harness configuration repository: a git repository with agents/<name>.toml, agents/<name>.md, skills/, rules.md
# and mcp.toml, cloned to data_dir/harness. Agents and rules then come from it, edits through the API are commits to it,
# and [agents.*] and rules_file are errors.
# [harness]
# url = "https://github.com/acme/harness-configuration"
# mcp_env = ["DOCS_TOKEN"]               # the environment variables mcp.toml may take values from, as ${NAME}

# Agents run on Pi-Bolt. The default workflow starts the Coder, and the Reviewer when there is one. An agent whose
# harness is "claude-code" runs on Claude Code; it speaks the anthropic protocol.
# [agents.Coder]
# model = "qwen3.8-flash-next"
# protocol = "openai"                     # anthropic for an agent on claude-code
# harness = "pi-bolt"                     # or claude-code
# thinking = "medium"
# runner = "gpu-box"                      # a [[runners]] name; without it the agent's runs go on the server
# instructions = "Keep changes small and run the tests."
# token_budget = 2000000
# engineering_skills = true               # false: not given the engineering skills Moebius brings (Coder, Reviewer, Grilling-partner)
# How the outputs of its tools are kept small before the model reads them:
# [agents.Coder.tool_output]
# hide_after_turns = 10                   # outputs of older turns are hidden behind a placeholder; 0 keeps them all
# max_lines = 200                         # one result's budget: above it, its head and tail; 0 is no limit
# max_bytes = 16384
# compress = true                         # compress test runs and build logs, strip escape codes and progress bars
"#;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::config::Config;

    #[test]
    fn the_example_is_exactly_the_defaults() {
        let parse = |text| Config::parse(text, Path::new("/srv"), Path::new("/srv/moebius.toml"));
        assert_eq!(parse(EXAMPLE).unwrap(), parse("").unwrap());
    }
}
