use crate::types::{DataKey, Error, HelpRequest, HelpRequestStatus};
use crate::utils::with_reentrancy_guard;
use soroban_sdk::{contract, contractevent, contractimpl, panic_with_error, token, Address, Env};

#[contract]
pub struct MutualAidContract;

#[contractevent]
pub struct HelpRequestPosted {
    request_id: u64,
    creator: Address,
    goal: i128,
}

/// Emitted after a donation is escrowed and request totals are updated. Topics are
/// `aid`, `donate`, and `request_id`; data is `[donor, amount_donated, new_total_raised]`.
#[contractevent(topics = ["aid", "donate"], data_format = "vec")]
pub struct DonationReceived {
    #[topic]
    request_id: u64,
    donor: Address,
    amount_donated: i128,
    new_total_raised: i128,
}

#[contractevent]
pub struct RefundClaimed {
    request_id: u64,
    donor: Address,
    amount: i128,
}

#[contractevent]
pub struct RequestCancelled {
    request_id: u64,
    creator: Address,
}

/// Emitted after a creator withdraws the funds raised by their request. Topics are
/// `aid`, `claim`, and `request_id`; data is `[creator, amount]`.
#[contractevent(topics = ["aid", "claim"], data_format = "vec")]
pub struct HelpRequestFundsClaimed {
    #[topic]
    request_id: u64,
    creator: Address,
    amount: i128,
}

#[contractimpl]
impl MutualAidContract {
    pub fn get_request(env: Env, request_id: u64) -> Option<HelpRequest> {
        let request_key = DataKey::HelpRequest(request_id);
        env.storage().persistent().get(&request_key)
    }

    pub fn post_help_request(
        env: Env,
        creator: Address,
        request_id: u64,
        goal: i128,
        token: Address,
    ) -> u64 {
        creator.require_auth();

        if goal <= 0 {
            panic_with_error!(&env, Error::InvalidGoalAmount);
        }

        // Prevent overwriting an existing request
        let request_key = DataKey::HelpRequest(request_id);
        if env.storage().persistent().has(&request_key) {
            panic_with_error!(&env, Error::HelpRequestAlreadyExists);
        }

        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token,
            goal,
            raised_amount: 0,
            status: HelpRequestStatus::Open,
            is_verified: false,
        };

        env.storage().persistent().set(&request_key, &request);

        HelpRequestPosted {
            request_id,
            creator,
            goal,
        }
        .publish(&env);

        request_id
    }

    pub fn donate(env: Env, donor: Address, request_id: u64, amount: i128) {
        donor.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidDonationAmount);
        }

        let request_key = DataKey::HelpRequest(request_id);
        let mut request: HelpRequest = env
            .storage()
            .persistent()
            .get(&request_key)
            .unwrap_or_else(|| panic_with_error!(&env, Error::HelpRequestNotFound));

        if request.status == HelpRequestStatus::FullyFunded {
            panic_with_error!(&env, Error::HelpRequestAlreadyFullyFunded);
        }

        // `Closed` is terminal: the creator has already withdrawn the escrow, so a
        // later donation could never be paid out or refunded.
        if request.status == HelpRequestStatus::Cancelled
            || request.status == HelpRequestStatus::Suspended
            || request.status == HelpRequestStatus::Closed
        {
            panic_with_error!(&env, Error::InvalidStatus);
        }

        let token_client = token::Client::new(&env, &request.token);

        token_client.transfer(&donor, env.current_contract_address(), &amount);

        let donation_key = DataKey::Donation(request_id, donor.clone());
        let previous_donation: i128 = env.storage().persistent().get(&donation_key).unwrap_or(0);
        let new_donation = previous_donation
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, Error::ArithmeticOverflow));
        env.storage().persistent().set(&donation_key, &new_donation);

        let new_raised = request
            .raised_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, Error::ArithmeticOverflow));

        request.raised_amount = new_raised;

        if new_raised >= request.goal {
            request.status = HelpRequestStatus::FullyFunded;
        }

        env.storage().persistent().set(&request_key, &request);

        DonationReceived {
            request_id,
            donor,
            amount_donated: amount,
            new_total_raised: new_raised,
        }
        .publish(&env);
    }

    pub fn claim_refund(env: Env, donor: Address, request_id: u64) {
        donor.require_auth();

        let request_key = DataKey::HelpRequest(request_id);
        let request: HelpRequest = env
            .storage()
            .persistent()
            .get(&request_key)
            .unwrap_or_else(|| panic_with_error!(&env, Error::HelpRequestNotFound));

        if request.status != HelpRequestStatus::Cancelled {
            panic_with_error!(&env, Error::InvalidStatus);
        }

        let donation_key = DataKey::Donation(request_id, donor.clone());
        let amount: i128 = env.storage().persistent().get(&donation_key).unwrap_or(0);

        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidDonationAmount);
        }

        let token_client = token::Client::new(&env, &request.token);
        token_client.transfer(&env.current_contract_address(), &donor, &amount);

        // Reset donation amount to prevent double refund
        env.storage().persistent().set(&donation_key, &0i128);

        RefundClaimed {
            request_id,
            donor,
            amount,
        }
        .publish(&env);
    }

    /// Withdraw the escrowed donations of a funded help request to its creator.
    ///
    /// Only the request creator may call this, and only from a release state:
    /// `FullyFunded`, or `ResolvedRelease` once a dispute has been settled in the
    /// creator's favour. The whole `raised_amount` is paid out — mutual aid carries
    /// no protocol fee, unlike giveaway prizes.
    ///
    /// The request moves to `Closed` and a one-shot claim record is written, so the
    /// payout cannot be repeated.
    pub fn claim_help_request_funds(env: Env, creator: Address, request_id: u64) {
        creator.require_auth();

        with_reentrancy_guard(&env, || {
            let request_key = DataKey::HelpRequest(request_id);
            let mut request: HelpRequest = env
                .storage()
                .persistent()
                .get(&request_key)
                .unwrap_or_else(|| panic_with_error!(&env, Error::HelpRequestNotFound));

            if request.creator != creator {
                panic_with_error!(&env, Error::NotCreator);
            }

            if request.status != HelpRequestStatus::FullyFunded
                && request.status != HelpRequestStatus::ResolvedRelease
            {
                panic_with_error!(&env, Error::InvalidStatus);
            }

            let claimed_key = DataKey::HelpRequestClaimed(request_id);
            let already_claimed: bool = env
                .storage()
                .persistent()
                .get(&claimed_key)
                .unwrap_or(false);
            if already_claimed {
                panic_with_error!(&env, Error::AlreadyClaimed);
            }

            let amount = request.raised_amount;
            if amount <= 0 {
                panic_with_error!(&env, Error::InvalidDonationAmount);
            }

            let token_client = token::Client::new(&env, &request.token);
            token_client.transfer(&env.current_contract_address(), &creator, &amount);

            env.storage().persistent().set(&claimed_key, &true);

            request.status = HelpRequestStatus::Closed;
            env.storage().persistent().set(&request_key, &request);

            HelpRequestFundsClaimed {
                request_id,
                creator,
                amount,
            }
            .publish(&env);
        })
    }

    pub fn cancel_request(env: Env, creator: Address, request_id: u64) {
        creator.require_auth();

        let request_key = DataKey::HelpRequest(request_id);
        let mut request: HelpRequest = env
            .storage()
            .persistent()
            .get(&request_key)
            .unwrap_or_else(|| panic_with_error!(&env, Error::HelpRequestNotFound));

        if request.creator != creator {
            panic_with_error!(&env, Error::NotCreator);
        }

        if request.status != HelpRequestStatus::Open {
            panic_with_error!(&env, Error::InvalidStatus);
        }

        request.status = HelpRequestStatus::Cancelled;
        env.storage().persistent().set(&request_key, &request);

        RequestCancelled {
            request_id,
            creator,
        }
        .publish(&env);
    }
}
