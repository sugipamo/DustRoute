---
name: stripe-projects-cli
description: Use the Stripe Projects CLI in this repository to manage deploying and access to third party services.
---

# Stripe Projects CLI

This repository is initialized for the Stripe project "DustRoute".

# Workflow
0. Run `stripe projects llm-context` to get the LLM context for the project.
1. Start with `stripe projects status` or `stripe projects show` to inspect the current project, linked providers, and named resources.
2. Use `stripe projects catalog` or `stripe projects services` to browse available providers and services. When you know the provider, run `stripe projects catalog <provider> --json` or `stripe projects catalog <provider>` and copy the exact `<provider>/<service>` slug from the output.
3. Provision a resource with `stripe projects add <provider>/<service>`. Do not guess the `stripe projects add` argument. Run `stripe projects catalog <provider> --json` or `stripe projects catalog <provider>` and copy the exact `<provider>/<service>` slug before you run `stripe projects add`. Example: `stripe projects add databaseco/postgres --name primary-db`. Use `--name <resource>` to control the local resource name used by future resource commands and environment variable prefixes. If you omit `--name`, the CLI uses the provider/service default for the local resource name. When a service config field looks like a name, the CLI uses the current project name as the default value when that satisfies the field schema. Use `--config '<json>'` when the service requires configuration.
4. Review credentials with `stripe projects env`. Values are redacted by default, and you can use `stripe projects env --pull` to write them to local files. If named project environment commands are available, `stripe projects env --pull` writes credentials for the active environment to that environment's output file.
5. After a successful `stripe projects add`, summarize the result and suggest next steps:

   | Field | Value |
   |-------|-------|
   | Provider | `<provider name>` |
   | Service | `<service type>` |
   | Tier | `<tier>` |
   | Env vars | `<variable names only — never values>` |

   Then show a compact summary of the other services already provisioned on the project (from `stripe projects status`):

   **Already on this project:**

   | Provider | Service | Env var prefix |
   |----------|---------|----------------|
   | ProviderA | service-name (Tier) | `PREFIX_*` |
   | ProviderB | service-name (Tier) | `PREFIX_*` |

   Then suggest 3–5 complementary services from different categories in the catalog (e.g., if user added a database, suggest auth, hosting, or observability). Only reference services that actually appear in `stripe projects catalog --json` output — never fabricate commands or provider names. Use this human-friendly format without CLI commands or provider/service slugs:

   1. ProviderName (category) — short description of what it provides
6. For named environments, use `stripe projects env list` to see all environments and the active `*`, `stripe projects env create <environment> --output .env.<environment>` to create one, and `stripe projects env use <environment>` to switch the active environment.
7. Use `stripe projects env add <resource>` and `stripe projects env remove <resource>` to change resource membership for the active environment only. Use `stripe projects env add <variable> --variable --env-key <KEY>` and `stripe projects env remove <variable> --variable` to change project variable membership for the active environment only.

## Optional notes
* If necessary, you can also link a provider with `stripe projects link <provider>` directly. But `stripe projects add <provider>/<service>` will guide you through provider authentication when needed.

# Working Agreement
- Commands can be run from the project root or nested directories inside the project.
- Do not hand-edit CLI-managed files under `.projects` or the generated `.env` output.
- NEVER look at any files in the .projects directory. The CLI manages everything for you.
- NEVER look at `.env`. The CLI manages everything for you.

# Agent mode
- You can use the `--json` flag when structured output will make follow-up steps easier.
- When you need to build a provisioning command programmatically, prefer `stripe projects catalog <provider> --json` so you can copy the exact `<provider>/<service>` slug without guessing.
- Use `--non-interactive` to disable prompts across commands. When you do, pass fully specified arguments and companion flags like `--yes` when the command requires confirmation.

## Headless limitations
You CANNOT complete browser authentication alone. If a command exits with `BROWSER_AUTH_REQUIRED`, Run `stripe login --non-interactive --new-session` to print JSON with `browser_url`, `verification_code`, and `next_step`; present `browser_url` and `verification_code` to the user, then run the emitted `next_step` command to complete login before retrying. `--new-session` is required: without it the Stripe CLI prints "already logged in" and exits 0 without authenticating, and that exit 0 is not success. If the CLI rejects `--new-session` as an unknown flag (Stripe CLI older than 1.50.0), run the same command without that flag — on those versions it prints the same JSON handoff when no session exists. If any login attempt prints "already logged in" instead of JSON, stop and tell the user to run `stripe projects init` themselves in a terminal with browser access — a session already exists, so `stripe login` prints the same "already logged in" for them and cannot authenticate Projects either. Never retry the original command until a login has actually completed. Try the login once only: if the same check still fails after a login that completed successfully, another sign-in will not change it — stop and tell the user to run `stripe projects init` themselves in a terminal with browser access. If a command exits with `PROJECTS_SESSION_UNUSABLE`, A Stripe CLI session exists for this account, but Stripe Projects cannot read live-mode credentials from it. A person has to run `stripe projects init` in a terminal with browser access to finish authenticating Projects. Nothing you can run resolves this, so do not retry the command. If a command exits with `ACCOUNT_NOT_ELIGIBLE`, The current account isn’t eligible for Stripe Projects. `stripe projects switch-account` needs an interactive terminal, so you cannot switch accounts yourself. Report this to the user and ask them to run `stripe projects switch-account` in a terminal with browser access. Do not retry the command. If a command exits with `MERCHANT_MISMATCH`, `stripe projects switch-account` needs an interactive terminal, so you cannot switch accounts yourself. Report this to the user and ask them to run `stripe projects switch-account` in a terminal with browser access. Do not retry the command. If a command exits with `INVALID_API_KEY`, the STRIPE_API_KEY environment variable is set and unusable: either it is a key for the other mode (pass or drop `--test`, or change the key) or it could not authenticate (unset it or provide a valid key). This one is yours to fix and it is not a session problem, so read the message, fix the variable or the mode, and retry — do not run `stripe projects init` for it. Do NOT retry the original command until the blocker is resolved.

An exit code of 0 from a login or account command does not by itself mean the blocker cleared. Re-read the output: if it says you are already logged in, or that a command needs an interactive terminal, the blocker is still there and the only way forward is a person.

## Error codes
When a command fails, the error output includes a machine-readable code in parentheses. React to these programmatically:

| Code | Meaning | What to do |
|------|---------|------------|
| `BROWSER_AUTH_REQUIRED` | No Stripe session and browser auth needed | Run `stripe login --non-interactive --new-session`, give the user `browser_url` and `verification_code`, then run the emitted `next_step`. If `--new-session` is rejected as unknown (Stripe CLI older than 1.50.0), drop it and run the command again. "already logged in" with no JSON is a stop, not a retry: hand the user `stripe projects init`, never `stripe login`, which prints the same thing for them. Try it once: if the check still fails after a completed login, another sign-in will not change it — hand the user `stripe projects init` and stop |
| `PROJECTS_SESSION_UNUSABLE` | Stripe CLI session exists but Projects cannot read live-mode credentials from it | Report the message to the user and stop. Do NOT retry |
| `BROWSER_AUTH_TIMEOUT` | Browser auth did not complete in time | Ask the user to finish the browser flow, then retry |
| `ACCOUNT_NOT_ELIGIBLE` | Account not onboarded for Projects | `stripe projects switch-account` needs an interactive terminal, so you cannot switch accounts yourself. Report this to the user and ask them to run `stripe projects switch-account` in a terminal with browser access. Do not retry the command. |
| `INVALID_API_KEY` | STRIPE_API_KEY is set and unusable: a key for the other mode, or one that failed to authenticate | Follow the message: pass or drop `--test`, or unset/replace STRIPE_API_KEY, then retry |
| `TOS_ACCEPTANCE_REQUIRED` | Provider terms not accepted | Re-run with `--accept-tos --yes` |
| `PLAN_REQUIRED` | Service needs a plan provisioned first | Provision the plan listed in the error, then retry |
| `PROVIDER_NOT_LINKED` | Provider requires OAuth linking | Run `stripe projects link <provider>` (may need browser) |
| `JSON_REQUIRES_CONFIRMATION` | Interactive confirmation needed | Re-run with `--yes` |
| `MERCHANT_MISMATCH` | Logged-in account differs from project owner | `stripe projects switch-account` needs an interactive terminal, so you cannot switch accounts yourself. Report this to the user and ask them to run `stripe projects switch-account` in a terminal with browser access. Do not retry the command. |

# Full command reference
- `stripe projects status` — view project, providers, and services
- `stripe projects catalog [provider]` — browse available services (optionally for one provider) and copy exact `provider/service` slugs
- `stripe projects add <provider>/<service>` — provision a service
- `stripe projects add databaseco/postgres --name primary-db` — example add command you can copy and adapt
    - `--name <resource>` — custom local resource name for future commands and env var prefixes
    - `--config '<json>'` — service configuration that can be passed with `projects add`
    - `--provider-config '<json>'` — provider link configuration (e.g. region)
    - `--force-provider-relink` — force a fresh provider link request during `add`
- `stripe projects add @database` — browse services by category (interactive only)
- `stripe projects remove <resource>` — remove a provisioned resource
- `stripe projects rotate <resource>` — rotate credentials for a resource
- `stripe projects upgrade <resource>` — change a resource's service tier
- `stripe projects open <provider>` — open provider dashboard in browser
- `stripe projects link <provider>` — link/re-link a provider
- `stripe projects link <provider> --force` — force a fresh provider re-link request
- `stripe projects env` — list credentials (redacted)
- `stripe projects env --pull` — fetch credentials and write them to `.env`
- `stripe projects env list` — list named project environments and mark the active one with `*`
- `stripe projects env show` — show the active project environment
- `stripe projects env create <environment> --output .env.<environment>` — create a named environment and make it active
- `stripe projects env use <environment>` — switch the active project environment
- `stripe projects env add <resource>` — add an existing resource to the active environment
- `stripe projects env remove <resource>` — remove resource membership from the active environment
- `stripe projects variables set <name> --env-key <KEY> [--value <value>]` — store a backend-backed project variable and bind it to the active environment
- `stripe projects variables list` — list project variables and local environment bindings
- `stripe projects variables delete <name>` — delete a project variable and its local bindings
- `stripe projects env add <variable> --variable --env-key <KEY>` — bind an existing project variable to the active environment
- `stripe projects env remove <variable> --variable` — remove project variable membership from the active environment
- `stripe projects llm-context` — get provider-specific LLM guidance
- `stripe projects billing show` — view billing method
- `stripe projects billing add` — add or update billing method
- `stripe projects spend` — view charges on your account

# Companion plan services
Some deployable services require a companion **plan** service to be provisioned first (controls pricing tier/resource limits).

## Checking existing plans
Run `stripe projects status` to see provisioned plans. If the required plan is already active, no action needed — proceed directly with the deployable.

## Provisioning order
When adding a deployable that has component pricing and no plan is yet provisioned:
1. Identify the required plan via `stripe projects catalog <provider> --json` — look for plan-kind services that are parents of the target deployable.
2. Provision the plan: `stripe projects add <provider>/<plan-service> --accept-tos --yes`
3. Provision the deployable: `stripe projects add <provider>/<deployable> --accept-tos --yes`

The plan must be provisioned before the deployable. If you skip it, the CLI exits with `PLAN_REQUIRED` and lists the exact command to provision the missing plan.

# Billing
If you need to deploy paid services, use `stripe projects billing add` to configure payment, or `stripe projects billing show` to view your current method.

# Deployment
If you get asked to deploy your project, copy the following files to the remote host into the project root:
* .env
* .projects/state.json
* .projects/state.local.json

Deploying a project might require to provision a provider that offers compute or hosting, and you may need to download their CLI.

# Troubleshooting
- If a command fails, check the error code in the output (e.g. `(PLAN_REQUIRED)`) and consult the error codes table above.
- If a command fails unexpectedly, run `stripe projects status --json` to understand the current state.
- If a provider shows status `PENDING_AUTH` or `EXPIRED`, run `stripe projects link <provider>` to re-authenticate. Add `--force` if you need a fresh re-link request regardless of local state.
- If credentials seem stale, run `stripe projects rotate <resource>` then `stripe projects env --pull`.

<!-- stripe-projects-cli managed:249859cb7e0f -->
