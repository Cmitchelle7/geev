# Geev Core Smart Contract

This is the core smart contract for the Geev decentralized giveaway platform, built on the Stellar blockchain using Soroban.

## Overview

The Geev Core contract enables decentralized giveaway creation and management with transparent, trustless winner selection. It implements:

- **Giveaway Creation**: Create giveaways with customizable parameters
- **Participant Registration**: Users can enter giveaways before the deadline
- **Winner Selection**: Random selection using ledger-based PRNG after giveaway ends
- **Prize Claiming**: Winners can claim their prizes once selected

## Features

### Winner Selection
Selection method is stored on the giveaway at creation (`create_giveaway` defaults
to `Random`; use `create_giveaway_with_selection` to choose explicitly):

- **Random** (`pick_winner`): ledger PRNG after `end_time`
- **FirstCome** (`finalize_first_come_winners`): first `winner_count` entrants by
  registration order; provisional winners are marked during `enter_giveaway`
- **Manual** (`finalize_manual_winners`): creator or admin supplies the winner list
- **Merit** (`finalize_merit_winners`): top reputations among participants

All methods share the same claim lifecycle (`claim_prize` / `recover_unclaimed_prize`)
once status becomes `Claimable`.

## Contract Structure

### Core Types

```rust
// Giveaway status states
pub enum GiveawayStatus {
    Active,      // Accepting entries
    Claimable,   // Winners selected, prizes claimable
    Completed,   // All prizes claimed or recovered
    Suspended,   // Governance suspension
}

// Winner selection methods
pub enum SelectionMethod {
    Random,     // Random selection via pick_winner (default)
    Manual,     // Creator/admin picks winners
    Merit,      // Highest reputation participants
    FirstCome,  // First winner_count entrants by registration order
}
```

### Storage Keys

- `Giveaway(u64)` - Store/retrieve giveaways by ID
- `ParticipantIndex(u64, u32)` - Map participant index to address
- `GiveawayCounter` - Generate unique IDs
- `HasEntered(u64, Address)` - Prevent double entry
- `Claimed(u64, Address)` - Per-winner claim record

## Functions

### `create_giveaway` / `create_giveaway_with_selection`
Create a new giveaway. `create_giveaway` stores `SelectionMethod::Random`.
`create_giveaway_with_selection` accepts an explicit `selection_method`.

### `enter_giveaway`
Add a participant to an active giveaway. For `FirstCome`, also appends the
participant to `winners` while slots remain (provisional until finalize).

### `pick_winner`
Select winners randomly when the giveaway period ends. Only valid when
`selection_method == Random`.

**Requirements:**
- Giveaway must be `Active`
- Current time must be after `end_time`
- `participant_count >= winner_count`

**Returns:** `Address` - First winner's wallet address

### `finalize_first_come_winners`
Lock in the first `winner_count` entrants (by `ParticipantIndex` order) after
`end_time`. Only valid when `selection_method == FirstCome`.

### `finalize_manual_winners` / `finalize_merit_winners`
Method-specific finalize paths for `Manual` and `Merit` giveaways (creator or admin).

### `claim_prize`
Allow a selected winner to claim their prize share while the giveaway is
`Claimable` and before `claim_deadline`.

**Requirements:**
- Giveaway status must be `Claimable`
- Claimer must be a selected winner who has not already claimed
- Claim window must not have expired

**Returns:** (void — transfers net prize after fee)
### `get_giveaway`
Retrieve giveaway details by ID.

**Parameters:**
- `giveaway_id: u64` - ID of the giveaway

**Returns:** `Option<Giveaway>` - Giveaway data or None

## Usage Examples

### Creating a Giveaway
```rust
let giveaway_id = GiveawayContract::create_giveaway(
    env.clone(),
    creator_address,
    "Free NFT Giveaway".to_string(),
    "Win one of 5 exclusive NFTs!".to_string(),
    "nft".to_string(),
    SelectionMethod::Random,
    5,           // 5 winners
    86400        // 24 hours duration
);
```

### Adding Participants
```rust
let entry_id = GiveawayContract::add_participant(
    env.clone(),
    giveaway_id,
    participant_address,
    "I'd love to win this NFT!".to_string()
);
```

### Selecting Winner (After End Time)
```rust
// Advance time beyond end_time
env.ledger().with_mut(|li| {
    li.timestamp = giveaway.end_time + 1000;
});

// Select winner
let winner_address = GiveawayContract::pick_winner(env, giveaway_id);
```

### Claiming Prize
```rust
let success = GiveawayContract::claim_prize(
    env, 
    giveaway_id, 
    winner_address
);
```

## Security Considerations

### MVP Implementation Limitations
⚠️ **Randomness Source**: Uses `env.prng()` which is ledger-based but not cryptographically secure. For production, consider using a more robust randomness source.

### Key Safeguards
- **Time-based Execution**: Winner selection only possible after `end_time`
- **Status Validation**: Prevents manipulation of completed giveaways
- **Participant Count**: Ensures at least one participant exists
- **Address Authentication**: Participants must authenticate their actions

## Testing

Run contract tests with:
```bash
cargo test
```

Tests cover:
- ✅ Giveaway creation
- ✅ Participant registration
- ✅ Winner selection with proper timing
- ✅ Error handling for edge cases
- ✅ Prize claiming functionality

## Error Handling

The contract defines specific error types:
```rust
pub enum Error {
    GiveawayNotFound = 1,
    GiveawayStillActive = 2,
    InvalidStatus = 3,
    NoParticipants = 4,
    NotCreator = 5,
    AlreadyCompleted = 6,
    InvalidIndex = 7,
    ParticipantAlreadyWinner = 8,
}
```

## Deployment

1. Build the contract:
```bash
soroban build
```

2. Deploy to testnet:
```bash
soroban deploy --network testnet
```

3. Deploy to local sandbox:
```bash
soroban deploy --network local
```

## Integration with Frontend

The contract is designed to integrate with the Geev frontend application:

- **API Alignment**: Contract methods mirror frontend concepts
- **Status Synchronization**: Giveaway states match frontend expectations
- **Error Compatibility**: Error codes map to user-friendly messages

## Future Enhancements

Planned improvements:
- 🔐 Cryptographically secure randomness
- 🔄 Multiple winner selection
- 📊 Merit-based selection algorithms
- ⚖️ Dispute resolution mechanisms
- 🛡️ Advanced participant verification