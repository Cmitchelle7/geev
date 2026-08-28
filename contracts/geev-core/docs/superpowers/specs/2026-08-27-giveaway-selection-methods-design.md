# Configurable Giveaway Selection Methods Design

Implements issue #403: "[Giveaway] Add configurable selection methods."

## Problem

Product docs (`SPECIFICATIONS.md`, `contracts/README.md`) describe random and
first-come winner selection. The contract already stored `SelectionMethod` and
implemented `Random`, `Manual`, and `Merit`, but not `FirstCome` — the additional
mode the product docs emphasize alongside random.

## Architecture

Keep one selection method per giveaway, chosen at creation:

| Method | Create default | Selection entry point | When winners lock |
|--------|----------------|----------------------|-------------------|
| `Random` | `create_giveaway` | `pick_winner` | After `end_time` |
| `Manual` | explicit | `finalize_manual_winners` | After `end_time` |
| `Merit` | explicit | `finalize_merit_winners` | After `end_time` |
| `FirstCome` | explicit | `finalize_first_come_winners` | After `end_time` |

Payout is unchanged: once status is `Claimable`, winners use `claim_prize` (and
creators/admins may `recover_unclaimed_prize` after the claim window).

## Data Model (`types.rs`)

Append only (preserve existing discriminants used by deployed/test data):

```rust
pub enum SelectionMethod {
    Random = 0,
    Manual = 1,
    Merit = 2,
    FirstCome = 3, // new
}
```

## FirstCome Behavior (`giveaway.rs`)

### During `enter_giveaway`
If `selection_method == FirstCome` and `winners.len() < winner_count`, append the
entrant to `winners`. Status stays `Active` so the campaign can run to `end_time`
and later entrants can still participate (they are not winners once slots are full).

### `finalize_first_come_winners(env, giveaway_id)`
- Requires `selection_method == FirstCome`, else `InvalidStatus`.
- Reuses `ensure_ready_for_selection` (Active, ended, enough participants).
- Builds winners from `ParticipantIndex(giveaway_id, 0..winner_count-1)`.
- Calls shared `finalize_winners` (emits `GiveawayWinnerSelected`, sets
  `Claimable` + `claim_deadline`).

Index order is authoritative at finalize time so the locked winner list matches
registration order even if the provisional `winners` vec were somehow stale.

## Testing

- `create_giveaway` stores `Random` by default.
- First-come entry marks the first `winner_count` addresses in `winners` while
  status remains `Active`.
- Finalize after end locks those entrants, emits winner events, sets `Claimable`.
- `pick_winner` rejects FirstCome; `finalize_first_come_winners` rejects Random.
- Finalize before `end_time` fails.
- First-come winners can `claim_prize` through the shared payout path.

## Out of Scope

- Closing the giveaway early when winner slots fill (entries stay open until
  `end_time`).
- Changing Manual/Merit behavior beyond coexistence with FirstCome.
- Frontend wiring for the new method.
