# Plan 2 Task 0: `claude -p` subscription route and usage spike

Date: 2026-10-20. Status: **INCOMPLETE, gate not decided.** Steps 1, 3 and part of 4 are done. Steps 2 (auth), 5 (measurement) and 6 (gate) are blocked: the installed CLI's login has expired and logging in is a user action.

## Step 1: docs and terms (read 2026-10-20)

- `code.claude.com/docs/en/headless`: `--bare` skips OAuth/keychain and needs `ANTHROPIC_API_KEY`, so it is NOT usable on the subscription route. Without `--bare`, `-p` loads user and project settings, hooks and MCP like an interactive session. `--output-format json` returns `total_cost_usd`, `usage`, `session_id` (client-side estimates), and `structured_output` with `--json-schema`. `--permission-prompts none` needs CLI 2.1.259+.
- `support.claude.com/en/articles/15036540`: `claude -p` and the Agent SDK still count against subscription plan limits. The 2026-06-15 note says the announced billing change is paused. The 2026-10-07 note says Max and Team plans also include monthly API credits for the Agent SDK/API.
- `support.claude.com/en/articles/17154008` (monthly API credits): Max 5x $100/mo, Max 20x $200/mo, Team per-seat. The credits cover `claude -p` only when run with an API key from the linked Console org. `claude -p` signed in with the plan uses plan usage limits instead. At credit exhaustion API requests stop until the next grant unless the org has purchased credits or auto-reload. Credits are separate from plan limits.
- `anthropic.com/legal/consumer-terms` s.3 item 7: automated or non-human access (bots, scripts) is barred except via an API key or where Anthropic explicitly permits it. s.2: no sharing credentials.
- `code.claude.com/docs/en/legal-and-compliance`: OAuth is for ordinary use of Claude Code and native Anthropic apps. Developers building products must use API keys; they may not offer claude.ai login, route requests through plan credentials for their users, or collect/store claude.ai credentials. An end user signing in to the unmodified Claude Code binary with their own subscription is explicitly not restricted.

Answers:
- (a) `claude -p` on plan login draws on plan usage limits (not the separate credit). The credit applies only when an API key is used.
- (b) A local app that spawns the unmodified official CLI under the user's own login, never touching tokens, is consistent with the above and with Anthropic's own docs for `-p`. It is a judgment call, not an explicit green light: consumer-terms item 7 allows automation only where Anthropic explicitly permits it, and the support article is the closest explicit statement. Do not store or proxy credentials, do not modify the binary, and do not pass plan access on to other people.
- (c) Exhaustion: plan limits give a rate-limit error from the CLI (not verified here); on the credit route requests stop until the next grant.

## Step 2: auth (blocked)

`claude --version` = 2.1.240 (brief wants >= 2.1.259; not updated by instruction). `claude auth status`: `loggedIn: false`, `authMethod: none`, text form says "Login: Expired". Every `claude -p` call returns `is_error: true`, `terminal_reason: api_error`, zero tokens. The user must run `claude auth login`.

## Step 4: flags on 2.1.240

Checked by invoking each flag from an empty cwd (argument parsing happens before the auth failure).
- Accepted: `--strict-mcp-config`, `--tools`, `--setting-sources`, `--disable-slash-commands`, `--permission-mode dontAsk`, `--system-prompt-file`, `--max-turns`, `--max-budget-usd`, `--session-id`, `--resume` (needs a real session), `--add-dir`, `--json-schema`, `--allowedTools`.
- **Unsupported on 2.1.240:** `--permission-prompts` ("unknown option"; needs 2.1.259+). The runner adds it only on CLI >= 2.1.259.
- Not yet verified (needs auth): contents of the `system/init` event (plugins, MCP servers, tool list), whether `--setting-sources local` suppresses user hooks and `~/.claude/CLAUDE.md`, and whether `--max-turns` works with `--json-schema`. Run `run.ps1 -Probe` once logged in; it saves the init event to `out/init.json`.

Candidate final flag list (to confirm): `-p --model <alias> --strict-mcp-config --tools Read --add-dir <out> --setting-sources local --disable-slash-commands --permission-mode dontAsk --allowedTools Read --system-prompt-file <file> --max-turns N --max-budget-usd N --session-id <uuid> --json-schema <schema> --output-format json` (+ `--permission-prompts none` on >= 2.1.259). Never `--bare`. Pass `--resume <id>` (without `--session-id`) for the crops pass. On Windows PowerShell 5.1, embedded quotes in the `--json-schema` argument must be escaped as `\"`.

## Step 5: measurement (not run)

Run each of: sonnet, opus x without/with `-Crops`, e.g. `.\run.ps1 -Source <folder of camera JPGs> -Model sonnet -Crops`. `out/runs.jsonl` gets one line per run. Meter delta (claude.ai Settings > Usage): pending, user to read.

| run | tokens in/out/cache | total_cost_usd | wall | turns | meter delta |
|---|---|---|---|---|---|
| sonnet, no crops | not run | | | | pending |
| sonnet, crops | not run | | | | pending |
| opus, no crops | not run | | | | pending |
| opus, crops | not run | | | | pending |

## Step 6: gate

Not decided. If the user's meter shows <= 10% of allowance for the crops run, `transport = cli-subscription`. Defaults per the brief: `helper_max_budget_usd` = 2x measured cost rounded up to $0.50, `helper_max_turns` = 2x measured turns, `helper_model` = cheaper model unless its picks were clearly worse.
