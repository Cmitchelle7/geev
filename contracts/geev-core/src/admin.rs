use crate::types::{DataKey, Error, GiveawayStatus, HelpRequestStatus};
use crate::{access::check_admin, types::HelpRequest};
use soroban_sdk::{contract, contractevent, contractimpl, panic_with_error, token, Address, Env};

#[contract]
pub struct AdminContract;

#[contractevent]
pub struct EmergencyWithdraw {
    token: Address,
    amount: i128,
    to: Address,
}

#[contractevent]
pub struct TokenAdded {
    token: Address,
}

#[contractevent]
pub struct RequestVerificationChanged {
    request_id: u64,
    is_verified: bool,
}

#[contractevent]
pub struct AppealResolved {
    #[topic]
    target_id: u64,
    restored: bool,
}

/// Emitted when admin control is transferred. Topics are `admin`, `transfer`,
/// plus the previous admin address; data is `[new_admin]`.
#[contractevent(topics = ["admin", "transfer"], data_format = "vec")]
pub struct AdminTransferred {
    #[topic]
    previous_admin: Address,
    new_admin: Address,
}

#[contractimpl]
impl AdminContract {
    /// Emergency withdraw function - callable only by Admin
    /// Allows rescuing funds in case of critical bugs, exploits, or migration needs
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `token` - The token address to withdraw
    /// * `amount` - The amount to withdraw
    /// * `to` - The safe address to send funds to
    ///
    /// # Panics
    /// Panics if called by non-admin address
    pub fn admin_withdraw(env: Env, token: Address, amount: i128, to: Address) {
        // Check admin authentication
        check_admin(&env);

        // Initialize Token Client
        let token_client = token::Client::new(&env, &token);

        // Execute transfer: From contract -> to
        token_client.transfer(&env.current_contract_address(), &to, &amount);

        // Emit EmergencyWithdraw event
        EmergencyWithdraw {
            token: token.clone(),
            amount,
            to: to.clone(),
        }
        .publish(&env);
    }

    /// Add a token to the whitelist - callable only by Admin
    /// Allows specific tokens to be used for giveaway creation
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `token` - The token address to whitelist
    ///
    /// # Panics
    /// Panics if called by non-admin address
    pub fn add_token(env: Env, token: Address) {
        // Check admin authentication
        check_admin(&env);

        // Add token to whitelist
        let token_key = DataKey::AllowedToken(token.clone());
        env.storage().instance().set(&token_key, &true);

        // Emit TokenAdded event
        TokenAdded { token }.publish(&env);
    }

    pub fn toggle_request_verification(env: Env, request_id: u64) {
        check_admin(&env);

        let request_key = DataKey::HelpRequest(request_id);
        let mut request: HelpRequest = env
            .storage()
            .persistent()
            .get(&request_key)
            .unwrap_or_else(|| panic_with_error!(&env, Error::HelpRequestNotFound));

        request.is_verified = !request.is_verified;

        env.storage().persistent().set(&request_key, &request);
        RequestVerificationChanged {
            request_id,
            is_verified: request.is_verified,
        }
        .publish(&env);
    }

    /// Resolve an appeal for suspended content - callable only by Admin
    /// Allows admin to restore the content or keep it suspended
    pub fn resolve_appeal(env: Env, target_id: u64, restore: bool) {
        check_admin(&env);

        let giveaway_key = DataKey::Giveaway(target_id);
        let request_key = DataKey::HelpRequest(target_id);

        let mut resolved = false;

        // Try Giveaway first.
        if let Some(mut giveaway) = env
            .storage()
            .persistent()
            .get::<DataKey, crate::types::Giveaway>(&giveaway_key)
        {
            if giveaway.status == GiveawayStatus::UnderAppeal {
                giveaway.status = if restore {
                    GiveawayStatus::Active
                } else {
                    GiveawayStatus::Suspended
                };
                env.storage().persistent().set(&giveaway_key, &giveaway);
                resolved = true;
            }
        }

        // Try HelpRequest if giveaway wasn't found/resolved.
        if !resolved {
            if let Some(mut request) = env
                .storage()
                .persistent()
                .get::<DataKey, crate::types::HelpRequest>(&request_key)
            {
                if request.status == HelpRequestStatus::UnderAppeal {
                    request.status = if restore {
                        HelpRequestStatus::Open
                    } else {
                        HelpRequestStatus::Suspended
                    };
                    env.storage().persistent().set(&request_key, &request);
                    resolved = true;
                }
            }
        }

        if resolved {
            AppealResolved {
                target_id,
                restored: restore,
            }
            .publish(&env);
        }
    }

    /// Transfer admin role to a new address - callable only by the current Admin
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `current_admin` - The address of the current admin (must match stored admin)
    /// * `new_admin` - The address that will become the new admin
    ///
    /// # Panics
    /// Panics if called by a non-admin address or if `current_admin` does not match storage
    pub fn transfer_admin(env: Env, current_admin: Address, new_admin: Address) {
        let admin = check_admin(&env);

        if admin != current_admin {
            panic_with_error!(&env, Error::NotAdmin);
        }

        env.storage().instance().set(&DataKey::Admin, &new_admin);

        AdminTransferred {
            previous_admin: current_admin,
            new_admin,
        }
        .publish(&env);
    }
}
