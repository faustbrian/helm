# Log Message Guidelines

This project uses structured CLI logs. Messages MUST be human readable and
consistent in tone, style, and wording.

## Tone

- Use clear, neutral language.
- Prefer plain English over shorthand and internal jargon.
- State what happened and why it matters.
- Avoid blame-oriented or emotional wording.

## Style

- Use sentence case.
- Keep messages concise and action-oriented.
- Avoid trailing periods unless the message contains multiple sentences.
- Prefer active voice.
- Include a reason when reporting a skip/failure.

## Structure

Use these level-specific patterns:

- `INFO`: Progress and state updates.
  - Pattern: `<Action> <object> ...`
  - Example: `Recreating service on random port 57105`
- `SUCCESS`: Completed actions.
  - Pattern: `<Action> <object>`
  - Example: `Started container 'acme-bill-db'`
- `WARN`: Non-fatal issues or skips.
  - Pattern: `Skipped <action> because <reason>`
  - Example: `Skipped container CA trust because container_port is 80`
- `ERROR`: Failures with context.
  - Pattern: `<Action> failed: <reason>`
  - Example: `Serve container failed: port is already allocated`

## Recommended Vocabulary

- Prefer:
  - `Starting`, `Running`, `Recreating`, `Removing`, `Stopping`
  - `Started`, `Completed`, `Removed`, `Stopped`
  - `Skipped ... because ...`
  - `... failed: ...`
- Avoid:
  - Ambiguous status-only phrasing such as `Done` or `OK`
  - Inconsistent synonyms for the same action in nearby code paths

## Scope And Tokens

- Scope and service tags are added by the logging layer.
- Message bodies SHOULD focus on meaningful action/result text.
- Do not manually add channel tokens (`[out]`/`[err]`) in message text.

## Examples

- Good:
  - `Recreating service on random port 57029`
  - `Purged container 'acme-postal-db'`
  - `Target completed successfully`
  - `Skipped CA trust because no app service is configured`
  - `Docker check failed: docker daemon is not reachable`

- Avoid:
  - `recreating service on random port 57029.`
  - `FAILED TO START`
  - `done`
  - `Error`
