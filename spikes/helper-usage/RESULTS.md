# Plan 2 Task 0: `claude -p` subscription route and usage spike

Date: 2026-10-20. Status: **complete, gate PROVISIONAL (plan usage meter reading still pending from the user).** Measured on CLI 2.1.240, Claude Pro plan.

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

## Step 2: auth

`claude --version` = 2.1.240 (brief wants >= 2.1.259; not updated by instruction). `claude auth status`: loggedIn true, `authMethod: claude.ai`, subscription type Pro. The CLI login had expired earlier and the user re-ran `claude auth login`.

Finding: a measurement run failed with 0 tokens because the account's 5-hour session limit had been reached. On Pro, interactive dev sessions and helper runs share the same 5-hour allowance, so a helper run can be starved by (or starve) normal Claude use.

## Step 4: flags and isolation (10-photo probe, stream-json)

- Accepted on 2.1.240: `--strict-mcp-config`, `--tools`, `--setting-sources`, `--disable-slash-commands`, `--permission-mode dontAsk`, `--system-prompt-file`, `--max-turns`, `--max-budget-usd`, `--session-id`, `--resume`, `--add-dir`, `--json-schema`, `--allowedTools`.
- **Unsupported on 2.1.240:** `--permission-prompts` (unknown option, needs 2.1.259+). The runner adds it only on newer CLIs; `dontAsk` already denies unlisted tools.
- `system/init` with the flags below: `plugins: []`, `mcp_servers: []`, `skills: []`, `slash_commands: []`, tools = `Read` plus the internal `StructuredOutput`, `permissionMode: dontAsk`, `apiKeySource: none` (subscription login works without `--bare`). The five built-in agents (general-purpose, Explore, Plan, statusline-setup, claude) are still listed.
- `--setting-sources local` suppressed the user-level enabled plugins (several are enabled in user settings, none loaded). The test machine has no user-level hooks and no user `CLAUDE.md`, so hook suppression and CLAUDE.md leakage are untested.
- **Leak found:** the user's auto-memory index still reached the child (it answered YES and quoted the first entry title). Setting `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` in the child's environment fixed it (answered NO). run.ps1 now sets it. The runner should too.
- `--max-turns 40` was passed but the 372-photo run reported `num_turns: 137` and completed, so `num_turns` evidently does not equal the `--max-turns` unit, or the cap was not enforced under `--json-schema`. Treat `helper_max_turns` as unreliable on 2.1.240 and rely on `--max-budget-usd` as the real cap.

Final flag list (verified on 2.1.240, run from an empty cwd): `claude -p --model <alias> --strict-mcp-config --tools Read --add-dir <out> --setting-sources local --disable-slash-commands --permission-mode dontAsk --allowedTools Read --system-prompt-file <file> --max-turns N --max-budget-usd N --session-id <uuid> --json-schema <schema> --output-format json`, env `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, plus `--permission-prompts none` on CLI >= 2.1.259. Use `--resume <id>` (without `--session-id`) for a second pass. Never `--bare` (needs an API key). Windows PowerShell 5.1 needs embedded quotes in `--json-schema` escaped as `\"`.

### Baseline context cost (trivial prompt "say ok", sonnet)

| config | cache-creation tokens | cost |
|---|---|---|
| default flags | 55,331 | $0.221 |
| isolated, `--tools Read` | 1,592 | $0.007 |
| isolated, `--tools ""` | about 0 (334 uncached input) | $0.001 |

Isolation flags cut the fixed per-run context from about 55k tokens to about 1.6k (a 97% reduction). Most of the saving is `--system-prompt-file` plus the default tool set, memory and plugin context.

## Step 5: measurement (one 372-photo shoot, sonnet, no crops)

Per the controller's ruling to protect the Pro allowance, only one measurement was run. Opus (both variants) and the real sonnet `-Crops` run were **skipped** for that reason, so there is no sonnet-vs-opus output comparison.

| run | tokens in / out / cache-write / cache-read | total_cost_usd | wall | turns | meter delta |
|---|---|---|---|---|---|
| probe, 10 photos | 6 / 2,058 / 8,197 / 11,942 | 0.057 | 21 s | 12 | n/a |
| sonnet, no crops, 372 photos in 7 moments | 22 / 19,730 / 65,577 / 439,887 | 0.566 | 168 s | 137 | pending: user to read before/after |
| sonnet, crops | skipped, estimated below | about 1.4 (est.) | about 4-5 min (est.) | about 150 (est.) | pending |
| opus, no crops | skipped (protect Pro allowance) | | | | |
| opus, crops | skipped (protect Pro allowance) | | | | |

Output of the measured run: 12 album picks, 7 your-call pairs, 30 shortlisted ids, `terminal_reason: completed`.

Crops estimate: source JPGs are 7728 x 5152. A 1024 px crop is 1024 x 683, about 932 image tokens (w*h/750), so 30 crops add about 28k tokens, versus about 53k for the 372 thumbnails (400 x 267, about 142 tokens each; matches the 65k cache-write seen). The second pass re-reads the whole conversation (about 95k tokens) every turn, so I estimate +$0.6 to +$1.2 over the no-crops run, point estimate $0.8, giving about $1.4 total. This is an extrapolation, not a measurement.

## Step 6: gate (PROVISIONAL, pending user meter reading)

- `transport = cli-subscription`, provisionally. The cost is small ($0.57 measured; about $1.4 estimated with crops, client-side estimates), but the pass criterion (<= 10% of the allowance) cannot be confirmed until the user supplies the usage-meter delta for the measured run. Pro limits are small and shared with dev sessions (see the 5-hour limit finding), so the user may prefer `cli-api-key` if the delta is large.
- `helper_model = sonnet` (cheaper). Not compared against opus (skipped), so no evidence yet on pick quality.
- `helper_max_budget_usd = 3.00` (2x the estimated with-crops cost of $1.4 = $2.80, rounded up to $0.50).
- `helper_max_turns = 300` (2x the estimated 150 turns for a crops run; see the `--max-turns` caveat above, the budget cap is the effective guard).
- Terms: acceptable for personal use per Step 1, with the caveats listed there.
