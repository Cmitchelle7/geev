# Giveaway Claim Timeout & Creator Recovery Design

Implements issue #406: "[Giveaway] Add unclaimed prize timeout and creator
recovery." Also absorbs the stated dependency, "Replace public distribution
with winner claim lifecycle," which had not landed yet — this branch builds
both in one PR since there was nothing to layer the timeout onto otherwise.

## Problem

Once a giveaway becomes `Claimable`, the existing `distribute_prize` pushes
funds to every winner in a single call. There's no per-winner claim step and
no deadline, so there's nothing to time out — a giveaway can only get stuck
if `distribute_prize` itself is never called, and even then there's no
recovery path.

## Architecture

Replace push-based `distribute_prize` with a pull-based claim lifecycle:

1. Winners individually call `claim_prize` for their own share while the
   giveaway is `Claimable` and before `claim_deadline`.
2. `claim_deadline` is a fixed duration (`CLAIM_WINDOW_SECONDS`, 7 days) after
   the giveaway becomes `Claimable`, set once in `finalize_winners`.
3. After the deadline, the creator or the contract admin calls
   `recover_unclaimed_prize`, which sweeps any still-unclaimed shares back to
   the creator and finalizes the giveaway.

## Data Model Changes (`types.rs`)

- `Giveaway` gains:
  - `claim_deadline: u64` — ledger timestamp after which claims are rejected.
  - `claimed_count: u32` — number of winners who have claimed so far.
- New `DataKey::Claimed(u64, Address) -> bool` — per-winner claim record,
  keyed by `(giveaway_id, winner_address)`.
- New `Error` variants (continuing from `NotAuthorizedResolver = 25`):
  - `ClaimWindowExpired = 26`
  - `ClaimWindowNotExpired = 27`
  - `AlreadyClaimed = 28`
  - `NotWinner = 29`

## Share & Fee Math

Each winner's gross share of `giveaway.amount` is computed with the same
even-split-plus-remainder logic `distribute_prize` used: `amount / winner_count`
per winner, with index 0 absorbing the integer-division remainder. This is
factored into a shared helper (`winner_gross_share`) used by both claim and
recovery paths so the math only lives in one place.

- **On claim:** the calling winner's `fee_bps` cut is deducted from their
  gross share and added to `CollectedFees(token)`; the remainder is
  transferred to the winner.
- **On recovery:** unclaimed shares are summed at their full gross value (no
  fee deducted) and transferred to the creator. The protocol only takes a
  fee on funds a winner actually claimed — recovered funds were never
  distributed, so there's nothing to take a cut of.

## Functions (`giveaway.rs`)

### `claim_prize(env: Env, giveaway_id: u64, winner: Address)`
- `winner.require_auth()`.
- Loads the giveaway; panics `GiveawayNotFound` if missing.
- Requires `status == Claimable`, else `InvalidStatus`.
- Requires `env.ledger().timestamp() <= claim_deadline`, else
  `ClaimWindowExpired`.
- Requires `winner` is present in `giveaway.winners`, else `NotWinner`.
- Requires `DataKey::Claimed(giveaway_id, winner)` is not already `true`,
  else `AlreadyClaimed`.
- Computes gross share via `winner_gross_share`, deducts `fee_bps`, transfers
  net to `winner`, adds fee to `CollectedFees(token)`.
- Marks `DataKey::Claimed(giveaway_id, winner) = true`, increments
  `claimed_count`.
- If `claimed_count == winners.len()`, sets `status = Completed` and
  increments the creator's reputation (`ProfileContract::increment_reputation`).
  Otherwise just persists the updated `claimed_count`.

### `recover_unclaimed_prize(env: Env, giveaway_id: u64, caller: Address)`
- `caller.require_auth()`.
- Loads the giveaway; panics `GiveawayNotFound` if missing.
- Requires `caller == giveaway.creator` or `caller == admin`, else `NotCreator`.
- Requires `status == Claimable`, else `InvalidStatus`.
- Requires `env.ledger().timestamp() > claim_deadline`, else
  `ClaimWindowNotExpired`.
- Sums `winner_gross_share` for every winner whose `Claimed` flag is not
  `true`; if the sum is zero there's nothing to recover (this only happens
  if every winner already claimed, which would already have flipped status
  to `Completed` in `claim_prize`, so in practice this path always has a
  positive amount).
- Transfers the summed amount to `giveaway.creator`.
- Sets `status = Completed` and persists.

### Removed: `distribute_prize`
Fully superseded by the two functions above — no push-based payout path
remains.

### Changed: `finalize_winners`
Adds one line where it currently sets `status = GiveawayStatus::Claimable`:
sets `claim_deadline = env.ledger().timestamp() + CLAIM_WINDOW_SECONDS` in
the same write.

## Testing (`test.rs`)

- Single-winner claim before expiry: correct net amount transferred, fee
  added to `CollectedFees`, status becomes `Completed`, reputation bumped.
- Multi-winner claim before expiry: each winner claims independently;
  giveaway only reaches `Completed` after the last claim.
- Claim after `claim_deadline` has passed: panics `ClaimWindowExpired`.
- Double claim by the same winner: panics `AlreadyClaimed`.
- Claim by an address not in `winners`: panics `NotWinner`.
- Recovery attempted before `claim_deadline`: panics `ClaimWindowNotExpired`.
- Recovery after expiry, called by creator: succeeds, unclaimed shares reach
  the creator, status becomes `Completed`.
- Recovery after expiry, called by a non-creator/non-admin address: panics
  `NotCreator`.
- Partial-claim recovery: one winner claims before expiry, one does not;
  after expiry, recovery transfers only the unclaimed winner's share to the
  creator, leaving the already-claimed winner's funds untouched.

## Out of Scope

- Redraw-winner or admin-supervised-dispute fallback policies (refund-only,
  per the approved design decision).
- Creator-configurable claim window length — `CLAIM_WINDOW_SECONDS` is a
  fixed constant for all giveaways.
- Any change to `pick_winner`, `finalize_manual_winners`, or
  `finalize_merit_winners` beyond the one `claim_deadline`-setting line
  shared via `finalize_winners`.
