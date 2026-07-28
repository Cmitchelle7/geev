# Mutual Aid Creator Withdrawal Design

Implements issue #407: "[MutualAid] Add creator withdrawal for fully funded help
requests."

## Problem

`donate` escrows donations in the contract and flips a help request to
`FullyFunded` once `raised_amount >= goal`, but no entry point ever moves that
escrow to the request creator. The only outbound path is `claim_refund`, which
requires a `Cancelled` request, and `cancel_request` requires an `Open` one — so
a fully funded request is a dead end where the collected support is stuck in the
contract forever.

## Architecture

Add a pull-based creator withdrawal, mirroring the giveaway `claim_prize`
lifecycle: the creator claims for themselves rather than any push-based payout.

1. The creator calls `claim_help_request_funds` once the request reached a
   release state.
2. The entire `raised_amount` is transferred to the creator.
3. The request becomes `Closed` and a one-shot claim record is written, which
   makes a second withdrawal impossible.

### Release states

A withdrawal is allowed from `FullyFunded` and from `ResolvedRelease`. The
latter is the documented dispute outcome in `HelpRequestStatus` meaning the
escrow was released to the creator; no entry point sets it yet, but accepting it
here means the future dispute-resolution flow needs no change to this function.
Every other status — including `Open` (goal not reached), `Cancelled` (escrow
belongs to the donors), `Suspended`, `Disputed`, and `UnderAppeal` — is
rejected with `InvalidStatus`.

### No protocol fee

Giveaway prizes take a `fee_bps` cut on claim. Mutual aid does not: a help
request collects support for a specific need, so the creator receives the full
`raised_amount`. This keeps the payout equal to the amount donors saw raised.

## Data Model Changes (`types.rs`)

- New `DataKey::HelpRequestClaimed(u64) -> bool` — one-shot payout record keyed
  by request id.

A request-scoped key is used rather than reusing `DataKey::Claimed(u64, Address)`
from the giveaway claim lifecycle: giveaway ids and help request ids come from
separate counters, so the same `(id, address)` pair can legitimately refer to
both a giveaway winner and a help request creator, and sharing the key would let
one record shadow the other.

No new `Error` variants are needed. The guards reuse `NotCreator`,
`InvalidStatus`, `HelpRequestNotFound`, `AlreadyClaimed`, and — for the
defensive zero-amount check — `InvalidDonationAmount`, matching how
`claim_refund` already reports an empty payout.

## Functions (`mutual_aid.rs`)

### `claim_help_request_funds(env: Env, creator: Address, request_id: u64)`
- `creator.require_auth()`, then the whole body runs inside
  `with_reentrancy_guard` because it transfers tokens out of the contract.
- Loads the request; panics `HelpRequestNotFound` if missing.
- Requires `request.creator == creator`, else `NotCreator`.
- Requires `status` is `FullyFunded` or `ResolvedRelease`, else `InvalidStatus`.
- Requires `HelpRequestClaimed(request_id)` is not already `true`, else
  `AlreadyClaimed`.
- Requires `raised_amount > 0`, else `InvalidDonationAmount`. Unreachable in
  practice (a release state implies `raised_amount >= goal > 0`) but it keeps
  the transfer from ever being called with a non-positive amount.
- Transfers `raised_amount` from the contract to the creator.
- Sets `HelpRequestClaimed(request_id) = true`, sets `status = Closed`, and
  persists the request.
- Emits `HelpRequestFundsClaimed`.

`raised_amount` is deliberately left intact rather than zeroed, so the request
keeps its funding history for indexers and the UI after payout.

### Changed: `donate`
Rejects donations to a `Closed` request with `InvalidStatus`, alongside the
existing `Cancelled` and `Suspended` rejections. Without this, a donation after
payout would be permanently trapped: it cannot be withdrawn again (the claim
record is set) and cannot be refunded (refunds require `Cancelled`, and
cancelling requires `Open`).

### New event: `HelpRequestFundsClaimed`
Topics `aid`, `claim`, and the request id; data `[creator, amount]` as a Vec,
following the existing `DonationReceived` shape so indexers can consume donation
and payout events the same way.

## Double-withdrawal Protection

Two independent guards:

1. The status transition to `Closed`, which no other entry point can undo —
   `cancel_request` and governance auto-suspension both require `Open`, and
   `resolve_appeal` requires `UnderAppeal`.
2. The `HelpRequestClaimed` record, which still blocks a payout even if a
   request were somehow returned to a release state.

## Testing (`test.rs`)

- Creator claims a fully funded request: full escrow transferred, status
  `Closed`, claim record set, `raised_amount` preserved.
- The payout emits `HelpRequestFundsClaimed` carrying the creator and amount.
- Second claim after a successful one: `InvalidStatus`.
- Claim record blocks a second payout even when the status is forced back to
  `FullyFunded`: `AlreadyClaimed`, with balances unchanged.
- Claim by a non-creator: `NotCreator`, escrow untouched.
- Claim of an under-funded (`Open`) request: `InvalidStatus`.
- Claim of a `Cancelled` request: `InvalidStatus`.
- Claim of a request that does not exist: `HelpRequestNotFound`.
- Claim from `ResolvedRelease`: succeeds and closes the request.
- Donation to a `Closed` request: `InvalidStatus`, donor keeps their tokens.
- No refund path after payout: `cancel_request` on a `Closed` request fails, so
  the escrow cannot be turned back into refundable donations.

## Out of Scope

- The dispute flow that would set `HelpRequestStatus::ResolvedRelease` /
  `ResolvedRefund` for help requests — only the release state is honoured here.
- Partial or milestone withdrawals; the payout is the whole raised amount in a
  single call.
- Creator withdrawal for requests that expire under their goal, and any deadline
  or timeout for claiming a funded request.
- Any protocol fee on mutual aid payouts.
