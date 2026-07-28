use crate::access::check_admin;
use crate::admin::{AdminContract, AdminContractClient};
use crate::giveaway::{GiveawayContract, GiveawayContractClient};
use crate::governance::{GovernanceContract, GovernanceContractClient};
use crate::mutual_aid::{MutualAidContract, MutualAidContractClient};
use crate::profile::{ProfileContract, ProfileContractClient};
use crate::types::{
    DataKey, Error, Giveaway, HelpRequest, HelpRequestStatus, ParticipantVerification,
};
use soroban_sdk::symbol_short;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, vec, Address, Env, FromVal, IntoVal, String, Symbol, Val, Vec,
};

#[test]
fn test_giveaway_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);

    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let title = String::from_str(&env, "Test Giveaway");
    let amount = 500;
    let duration = 60;

    let target_giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &amount,
        &title,
        &duration,
        &1,
        &None,
    );

    assert_eq!(token_client.balance(&creator), 500);
    assert_eq!(token_client.balance(&contract_id), 500);
    assert_eq!(target_giveaway_id, 1);

    contract_client.enter_giveaway(&user1, &target_giveaway_id);
    contract_client.enter_giveaway(&user2, &target_giveaway_id);

    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });

    let winner = contract_client.pick_winner(&target_giveaway_id);

    assert!(winner == user1 || winner == user2);

    let events = env.events().all();
    let expected_topics: soroban_sdk::Vec<Val> = vec![
        &env,
        symbol_short!("giveaway").into_val(&env),
        symbol_short!("winner").into_val(&env),
        winner.into_val(&env),
    ];
    assert!(events.iter().any(|(event_contract, topics, _data)| {
        event_contract == contract_id && topics == expected_topics.into_val(&env)
    }));
}

#[test]
#[should_panic]
fn test_allowlist_rejects_unverified_participant() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let allowed_user = Address::generate(&env);
    let blocked_user = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let mut allowlist: Vec<Address> = Vec::new(&env);
    allowlist.push_back(allowed_user.clone());

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Allowlist Giveaway"),
        &60,
        &1,
        &Some(ParticipantVerification {
            allowlist,
            min_reputation: 0,
            uses_reputation: false,
        }),
    );

    contract_client.enter_giveaway(&allowed_user, &giveaway_id);
    contract_client.enter_giveaway(&blocked_user, &giveaway_id);
}

#[test]
#[should_panic]
fn test_reputation_gated_giveaway_rejects_low_reputation() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let low_rep_user = Address::generate(&env);
    let high_rep_user = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(high_rep_user.clone()), &10u64);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Reputation Giveaway"),
        &60,
        &1,
        &Some(ParticipantVerification {
            allowlist: Vec::new(&env),
            min_reputation: 5,
            uses_reputation: true,
        }),
    );

    contract_client.enter_giveaway(&high_rep_user, &giveaway_id);
    contract_client.enter_giveaway(&low_rep_user, &giveaway_id);
}

#[test]
fn test_multi_winner_giveaway_selects_unique_winners() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant1 = Address::generate(&env);
    let participant2 = Address::generate(&env);
    let participant3 = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &400,
        &String::from_str(&env, "Multi Winner Giveaway"),
        &60,
        &2,
        &None,
    );

    contract_client.enter_giveaway(&participant1, &giveaway_id);
    contract_client.enter_giveaway(&participant2, &giveaway_id);
    contract_client.enter_giveaway(&participant3, &giveaway_id);

    env.ledger().with_mut(|li| li.timestamp += 100);

    let winner = contract_client.pick_winner(&giveaway_id);
    assert!(winner == participant1 || winner == participant2 || winner == participant3);

    let winners: Vec<Address> = env.as_contract(&contract_id, || {
        let giveaway: Giveaway = env
            .storage()
            .persistent()
            .get(&DataKey::Giveaway(giveaway_id))
            .unwrap();
        assert_eq!(giveaway.winners.len(), 2);
        let winner0 = giveaway.winners.get(0).unwrap();
        let winner1 = giveaway.winners.get(1).unwrap();
        assert!(winner0 != winner1);
        giveaway.winners.clone()
    });

    contract_client.claim_prize(&giveaway_id, &winners.get(0).unwrap());
    contract_client.claim_prize(&giveaway_id, &winners.get(1).unwrap());

    let winner1_balance = token::Client::new(&env, &mock_token).balance(&winners.get(0).unwrap());
    let winner2_balance = token::Client::new(&env, &mock_token).balance(&winners.get(1).unwrap());
    assert_eq!(winner1_balance + winner2_balance, 396);
    assert_eq!(
        token::Client::new(&env, &mock_token).balance(&contract_id),
        4
    );
}

#[test]
#[should_panic]
fn test_double_entry_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);

    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let greedy_user = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&greedy_user, &id);

    contract_client.enter_giveaway(&greedy_user, &id);
}

#[test]
#[should_panic]
fn test_enter_late_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);

    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let late_user = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Test"),
        &60,
        &1,
        &None,
    );

    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });

    contract_client.enter_giveaway(&late_user, &id);
}

#[test]
#[should_panic]
fn test_pick_winner_early_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);

    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let user = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&user, &id);

    contract_client.pick_winner(&id);
}

#[test]
fn test_donation_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let contract_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let donor1 = Address::generate(&env);
    let donor2 = Address::generate(&env);

    token_admin_client.mint(&donor1, &1000);
    token_admin_client.mint(&donor2, &1000);

    let request_id: u64 = 1;
    let goal = 1000;
    let donation1 = 300;
    let donation2 = 700;

    env.as_contract(&contract_id, || {
        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token: mock_token.clone(),
            goal,
            raised_amount: 0,
            status: HelpRequestStatus::Open,
            is_verified: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });

    assert_eq!(token_client.balance(&donor1), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);

    contract_client.donate(&donor1, &request_id, &donation1);

    assert_eq!(token_client.balance(&donor1), 700);
    assert_eq!(token_client.balance(&contract_id), 300);

    contract_client.donate(&donor2, &request_id, &donation2);

    assert_eq!(token_client.balance(&donor2), 300);
    assert_eq!(token_client.balance(&contract_id), 1000);

    env.as_contract(&contract_id, || {
        let request: HelpRequest = env
            .storage()
            .persistent()
            .get(&DataKey::HelpRequest(request_id))
            .unwrap();
        assert_eq!(request.raised_amount, goal);
        assert_eq!(request.status, HelpRequestStatus::FullyFunded);
    });
}

#[test]
fn test_donation_reaches_goal() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let contract_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let donor = Address::generate(&env);

    token_admin_client.mint(&donor, &2000);

    let request_id: u64 = 2;
    let goal = 500;
    let donation = 500;

    env.as_contract(&contract_id, || {
        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token: mock_token.clone(),
            goal,
            raised_amount: 0,
            status: HelpRequestStatus::Open,
            is_verified: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });

    contract_client.donate(&donor, &request_id, &donation);

    env.as_contract(&contract_id, || {
        let request: HelpRequest = env
            .storage()
            .persistent()
            .get(&DataKey::HelpRequest(request_id))
            .unwrap();
        assert_eq!(request.raised_amount, goal);
        assert_eq!(request.status, HelpRequestStatus::FullyFunded);
    });
}

#[test]
fn test_donation_emits_contributor_tracking_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let contract_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let donor = Address::generate(&env);

    token_admin_client.mint(&donor, &1000);

    let request_id: u64 = 42;
    let goal = 500;
    let donation = 125;

    env.as_contract(&contract_id, || {
        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token: mock_token.clone(),
            goal,
            raised_amount: 0,
            status: HelpRequestStatus::Open,
            is_verified: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });

    contract_client.donate(&donor, &request_id, &donation);

    let events = env.events().all();
    // Topics as `Val` + explicit `Vec<Val>` so `vec!` type-checks on soroban-sdk 23.5+.
    let expected_topics: soroban_sdk::Vec<Val> = vec![
        &env,
        symbol_short!("aid").into_val(&env),
        symbol_short!("donate").into_val(&env),
        request_id.into_val(&env),
    ];
    assert!(events.iter().any(|(event_contract, topics, _data)| {
        event_contract == contract_id && topics == expected_topics.into_val(&env)
    }));
}

#[test]
#[should_panic]
fn test_donation_to_nonexistent_request_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let contract_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let donor = Address::generate(&env);
    token_admin_client.mint(&donor, &1000);

    let nonexistent_request_id: u64 = 999;

    contract_client.donate(&donor, &nonexistent_request_id, &100);
}

#[test]
#[should_panic]
fn test_donation_to_fully_funded_request_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let contract_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let donor = Address::generate(&env);

    token_admin_client.mint(&donor, &1000);

    let request_id: u64 = 3;
    let goal = 500;

    env.as_contract(&contract_id, || {
        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token: mock_token.clone(),
            goal,
            raised_amount: goal,
            status: HelpRequestStatus::FullyFunded,
            is_verified: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });

    contract_client.donate(&donor, &request_id, &100);
}

#[test]
#[should_panic]
fn test_donation_with_invalid_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let contract_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let donor = Address::generate(&env);

    token_admin_client.mint(&donor, &1000);

    let request_id: u64 = 4;
    let goal = 500;

    env.as_contract(&contract_id, || {
        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token: mock_token.clone(),
            goal,
            raised_amount: 0,
            status: HelpRequestStatus::Open,
            is_verified: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });

    contract_client.donate(&donor, &request_id, &0);
}

#[test]
fn test_claim_prize() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let winner = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Prize Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&winner, &giveaway_id);

    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });

    let picked_winner = contract_client.pick_winner(&giveaway_id);
    assert_eq!(picked_winner, winner);

    assert_eq!(token_client.balance(&winner), 0);
    assert_eq!(token_client.balance(&contract_id), 500);

    contract_client.claim_prize(&giveaway_id, &winner);

    // Winner receives 99% (500 - 1% fee = 495)
    assert_eq!(token_client.balance(&winner), 495);
    // Contract retains 1% fee (5 tokens)
    assert_eq!(token_client.balance(&contract_id), 5);
}

#[test]
fn test_init_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_bps: u32 = 100;

    contract_client.init(&admin, &fee_bps);

    env.as_contract(&contract_id, || {
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let stored_fee: u32 = env.storage().instance().get(&DataKey::Fee).unwrap();

        assert_eq!(stored_admin, admin);
        assert_eq!(stored_fee, fee_bps);
    });
}

#[test]
#[should_panic]
fn test_claim_prize_wrong_status_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Prize Test"),
        &60,
        &1,
        &None,
    );

    contract_client.claim_prize(&giveaway_id, &creator);
}

#[test]
#[should_panic]
fn test_init_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_bps: u32 = 100;

    contract_client.init(&admin, &fee_bps);
    contract_client.init(&admin, &fee_bps);
}

#[test]
fn test_admin_withdraw_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let admin = Address::generate(&env);
    let safe_address = Address::generate(&env);

    // Initialize contract with admin
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Admin, &admin);
    });

    // Mint tokens to contract
    token_admin_client.mint(&contract_id, &1000);

    assert_eq!(token_client.balance(&contract_id), 1000);
    assert_eq!(token_client.balance(&safe_address), 0);

    // Admin withdraws funds
    contract_client.admin_withdraw(&mock_token, &500, &safe_address);

    assert_eq!(token_client.balance(&contract_id), 500);
    assert_eq!(token_client.balance(&safe_address), 500);
}

#[test]
#[should_panic]
fn test_admin_withdraw_fails_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let safe_address = Address::generate(&env);

    // DO NOT initialize contract with admin - this should cause panic

    // Mint tokens to contract
    token_admin_client.mint(&contract_id, &1000);

    // Try to withdraw without admin being initialized - should panic
    contract_client.admin_withdraw(&mock_token, &500, &safe_address);
}

#[test]
fn test_admin_withdraw_full_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let admin = Address::generate(&env);
    let safe_address = Address::generate(&env);

    // Initialize contract with admin
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Admin, &admin);
    });

    // Mint tokens to contract
    token_admin_client.mint(&contract_id, &5000);

    assert_eq!(token_client.balance(&contract_id), 5000);

    // Admin withdraws full balance
    contract_client.admin_withdraw(&mock_token, &5000, &safe_address);

    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(token_client.balance(&safe_address), 5000);
}

#[test]
fn test_refund_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let contract_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let donor = Address::generate(&env);

    token_admin_client.mint(&donor, &1000);

    let request_id: u64 = 10;
    let goal = 1000;
    let donation = 500;

    env.as_contract(&contract_id, || {
        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token: mock_token.clone(),
            goal,
            raised_amount: 0,
            status: HelpRequestStatus::Open,
            is_verified: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });

    // 1. Donate
    contract_client.donate(&donor, &request_id, &donation);
    assert_eq!(token_client.balance(&donor), 500);
    assert_eq!(token_client.balance(&contract_id), 500);

    // 2. Cancel request
    contract_client.cancel_request(&creator, &request_id);

    // 3. Claim refund
    contract_client.claim_refund(&donor, &request_id);

    // 4. Verify balances
    assert_eq!(token_client.balance(&donor), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);

    // 5. Verify donation reset
    env.as_contract(&contract_id, || {
        let donation_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Donation(request_id, donor.clone()))
            .unwrap_or(-1);
        assert_eq!(donation_amount, 0);
    });
}

#[test]
#[should_panic(expected = "reentrancy detected")]
fn test_claim_prize_reentrancy_protection() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);
    let creator = Address::generate(&env);
    let winner = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Prize Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&winner, &giveaway_id);

    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });

    contract_client.pick_winner(&giveaway_id);

    // Simulate the lock already being held before claim_prize is called
    // as if a reentrant call is in progress
    env.as_contract(&contract_id, || {
        env.storage().temporary().set(&symbol_short!("Lock"), &true);
    });

    // This should panic with "reentrancy detected" because the lock is already set
    contract_client.claim_prize(&giveaway_id, &winner);
}

#[test]
fn test_add_token_to_whitelist() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    // Initialize contract with admin
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Admin, &admin);
    });

    // Add token to whitelist
    contract_client.add_token(&token);

    // Verify token is whitelisted
    env.as_contract(&contract_id, || {
        let is_allowed: bool = env
            .storage()
            .instance()
            .get(&DataKey::AllowedToken(token.clone()))
            .unwrap_or(false);
        assert!(is_allowed);
    });
}

#[test]
#[should_panic]
fn test_add_token_fails_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let token = Address::generate(&env);

    // DO NOT initialize contract with admin - this should cause panic
    // Try to add token without admin being initialized - should panic
    contract_client.add_token(&token);
}

#[test]
fn test_create_giveaway_with_whitelisted_token() {
    let env = Env::default();
    env.mock_all_auths();

    let giveaway_contract_id = env.register(GiveawayContract, ());
    let giveaway_client = GiveawayContractClient::new(&env, &giveaway_contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    // Initialize both contracts with same admin
    env.as_contract(&giveaway_contract_id, || {
        env.storage().instance().set(&DataKey::Admin, &admin);
    });

    env.as_contract(&giveaway_contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    token_admin_client.mint(&creator, &1000);

    // Create giveaway with whitelisted token - should succeed
    let giveaway_id = giveaway_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Whitelisted Token Test"),
        &60,
        &1,
        &None,
    );

    assert_eq!(giveaway_id, 1);
}

#[test]
#[should_panic]
fn test_create_giveaway_with_non_whitelisted_token_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);

    token_admin_client.mint(&creator, &1000);

    // Try to create giveaway without whitelisting token - should panic with TokenNotSupported
    contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Non-Whitelisted Token Test"),
        &60,
        &1,
        &None,
    );
}

// ── Profile Registry tests ────────────────────────────────────────────────────

#[test]
fn test_set_and_get_profile() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ProfileContract, ());
    let client = ProfileContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "alice");
    let avatar = String::from_str(&env, "QmHash123");

    client.set_profile(&user, &username, &avatar);

    let profile = client.get_profile(&user).unwrap();
    assert_eq!(profile.username, username);
    assert_eq!(profile.avatar_hash, avatar);
}

#[test]
fn test_resolve_username_returns_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ProfileContract, ());
    let client = ProfileContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "bob");
    let avatar = String::from_str(&env, "QmAvatarBob");

    client.set_profile(&user, &username, &avatar);

    let resolved = client.resolve_username(&username).unwrap();
    assert_eq!(resolved, user);
}

#[test]
#[should_panic]
fn test_duplicate_username_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ProfileContract, ());
    let client = ProfileContractClient::new(&env, &contract_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let username = String::from_str(&env, "geev_user");
    let avatar = String::from_str(&env, "QmHash456");

    client.set_profile(&alice, &username, &avatar);
    client.set_profile(&bob, &username, &avatar);
}

#[test]
fn test_user_can_change_username_and_old_one_is_freed() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ProfileContract, ());
    let client = ProfileContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let old_username = String::from_str(&env, "old_name");
    let new_username = String::from_str(&env, "new_name");
    let avatar = String::from_str(&env, "QmHash789");

    client.set_profile(&user, &old_username, &avatar);
    client.set_profile(&user, &new_username, &avatar);

    assert!(client.resolve_username(&old_username).is_none());
    assert_eq!(client.resolve_username(&new_username).unwrap(), user);
}

#[test]
fn test_get_profile_returns_none_for_unknown_address() {
    let env = Env::default();
    let contract_id = env.register(ProfileContract, ());
    let client = ProfileContractClient::new(&env, &contract_id);

    let stranger = Address::generate(&env);
    assert!(client.get_profile(&stranger).is_none());
}

#[test]
fn test_check_admin_helper() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let admin = Address::generate(&env);

    // Initialize contract with admin
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Admin, &admin);
    });

    // Test that check_admin returns the admin address
    env.as_contract(&contract_id, || {
        let returned_admin = check_admin(&env);
        assert_eq!(returned_admin, admin);
    });
}

#[test]
#[should_panic]
fn test_check_admin_fails_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());

    // DO NOT initialize admin - should panic
    env.as_contract(&contract_id, || {
        check_admin(&env);
    });
}
#[test]
fn test_withdraw_fees() {
    let env = Env::default();
    env.mock_all_auths();

    let giveaway_contract_id = env.register(GiveawayContract, ());
    let giveaway_client = GiveawayContractClient::new(&env, &giveaway_contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let winner = Address::generate(&env);

    // Initialize giveaway contract
    giveaway_client.init(&admin, &100u32); // 1%

    env.as_contract(&giveaway_contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    token_admin_client.mint(&creator, &1000);

    // Create and complete a giveaway to generate fees
    let giveaway_id = giveaway_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Fee Test"),
        &60,
        &1,
        &None,
    );

    giveaway_client.enter_giveaway(&winner, &giveaway_id);

    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });

    giveaway_client.pick_winner(&giveaway_id);
    giveaway_client.claim_prize(&giveaway_id, &winner);

    // Verify fees were collected (5 tokens = 1% of 500)
    assert_eq!(token_client.balance(&giveaway_contract_id), 5);
    assert_eq!(token_client.balance(&winner), 495);
    assert_eq!(token_client.balance(&admin), 0);

    // Verify collected fees are tracked in giveaway contract
    env.as_contract(&giveaway_contract_id, || {
        let collected_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::CollectedFees(mock_token.clone()))
            .unwrap_or(0);
        assert_eq!(collected_fees, 5);
    });

    // Withdraw fees using giveaway contract
    giveaway_client.withdraw_fees(&mock_token);

    // Verify fees were transferred to admin and counter reset
    assert_eq!(token_client.balance(&admin), 5);
    assert_eq!(token_client.balance(&giveaway_contract_id), 0);

    env.as_contract(&giveaway_contract_id, || {
        let collected_fees: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::CollectedFees(mock_token.clone()))
            .unwrap_or(0);
        assert_eq!(collected_fees, 0);
    });
}

#[test]
#[should_panic]
fn test_withdraw_fees_fails_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let giveaway_contract_id = env.register(GiveawayContract, ());
    let giveaway_client = GiveawayContractClient::new(&env, &giveaway_contract_id);

    let token = Address::generate(&env);

    // DO NOT initialize admin - should panic
    giveaway_client.withdraw_fees(&token);
}

#[test]
fn test_toggle_request_verification() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token = Address::generate(&env);
    let request_id: u64 = 42;

    // Initialize admin
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Admin, &admin);
    });

    // Seed a help request with is_verified = false
    env.as_contract(&contract_id, || {
        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token: token.clone(),
            goal: 1000,
            raised_amount: 0,
            status: HelpRequestStatus::Open,
            is_verified: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });

    // Toggle to verified
    contract_client.toggle_request_verification(&request_id);

    env.as_contract(&contract_id, || {
        let request: HelpRequest = env
            .storage()
            .persistent()
            .get(&DataKey::HelpRequest(request_id))
            .unwrap();
        assert!(request.is_verified);
    });

    // Toggle back to unverified
    contract_client.toggle_request_verification(&request_id);

    env.as_contract(&contract_id, || {
        let request: HelpRequest = env
            .storage()
            .persistent()
            .get(&DataKey::HelpRequest(request_id))
            .unwrap();
        assert!(!request.is_verified);
    });
}

#[test]
#[should_panic]
fn test_toggle_request_verification_fails_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    // DO NOT initialize admin - should panic
    contract_client.toggle_request_verification(&1u64);
}

/// Verifies that `donate` emits a `DonationReceived` event whose data contains
/// the exact `amount_donated` for that call and the cumulative `new_total_raised`.
/// This directly covers the acceptance criteria:
///   "Emits the exact amount donated and the updated total."
#[test]
fn test_donate_event_emits_exact_amount_and_total() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let contract_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let donor1 = Address::generate(&env);
    let donor2 = Address::generate(&env);

    let first_amount: i128 = 250;
    let second_amount: i128 = 350;

    token_admin_client.mint(&donor1, &first_amount);
    token_admin_client.mint(&donor2, &second_amount);

    let request_id: u64 = 55;
    let goal: i128 = 1000;

    env.as_contract(&contract_id, || {
        let request = HelpRequest {
            id: request_id,
            creator: creator.clone(),
            token: mock_token.clone(),
            goal,
            raised_amount: 0,
            status: HelpRequestStatus::Open,
            is_verified: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });

    contract_client.donate(&donor1, &request_id, &first_amount);
    contract_client.donate(&donor2, &request_id, &second_amount);

    // After both donations the cumulative total must be first + second.
    let expected_total: i128 = first_amount + second_amount;

    let events = env.events().all();

    // Topics expected for the second DonationReceived event.
    let expected_topics: soroban_sdk::Vec<Val> = vec![
        &env,
        symbol_short!("aid").into_val(&env),
        symbol_short!("donate").into_val(&env),
        request_id.into_val(&env),
    ];
    assert!(
        events.iter().any(|(event_contract, topics, data)| {
            if event_contract != contract_id || topics != expected_topics.into_val(&env) {
                return false;
            }
            // Decode the data Vec and compare each field to its concrete type.
            let data_vec: soroban_sdk::Vec<Val> = soroban_sdk::Vec::from_val(&env, &data);
            let actual_donor = Address::from_val(&env, &data_vec.get(0).unwrap());
            let actual_amount = i128::from_val(&env, &data_vec.get(1).unwrap());
            let actual_total = i128::from_val(&env, &data_vec.get(2).unwrap());
            actual_donor == donor2
                && actual_amount == second_amount
                && actual_total == expected_total
        }),
        "DonationReceived event did not contain the exact amount_donated and new_total_raised"
    );
}

// ── Governance / flag_content tests ──────────────────────────────────────────
#[test]
fn test_flag_content_increments_count() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GovernanceContract, ());
    let client = GovernanceContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let target_id: u64 = 42;

    assert_eq!(client.get_flag_count(&target_id), 0);

    client.flag_content(&user, &target_id);

    assert_eq!(client.get_flag_count(&target_id), 1);
}

#[test]
fn test_flag_content_multiple_users() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GovernanceContract, ());
    let client = GovernanceContractClient::new(&env, &contract_id);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let target_id: u64 = 7;

    client.flag_content(&user_a, &target_id);
    client.flag_content(&user_b, &target_id);

    assert_eq!(client.get_flag_count(&target_id), 2);
    assert!(client.has_flagged(&user_a, &target_id));
    assert!(client.has_flagged(&user_b, &target_id));
}

#[test]
#[should_panic]
fn test_flag_content_duplicate_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GovernanceContract, ());
    let client = GovernanceContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let target_id: u64 = 1;

    client.flag_content(&user, &target_id);
    // Second flag from the same user must panic with AlreadyFlagged
    client.flag_content(&user, &target_id);
}

#[test]
fn test_has_flagged_returns_false_before_flag() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GovernanceContract, ());
    let client = GovernanceContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    assert!(!client.has_flagged(&user, &99u64));
}

#[test]
fn test_flag_counts_are_independent_per_id() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GovernanceContract, ());
    let client = GovernanceContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    client.flag_content(&user, &1u64);

    // ID 2 should still be at 0
    assert_eq!(client.get_flag_count(&2u64), 0);
    assert_eq!(client.get_flag_count(&1u64), 1);
}

// ── auto-suspension tests ─────────────────────────────────────────────────────

use crate::governance::FLAG_THRESHOLD;
use crate::types::{GiveawayStatus, SelectionMethod};

/// Seed a minimal active Giveaway directly into contract storage.
fn seed_active_giveaway(env: &Env, contract_id: &Address, giveaway_id: u64, token: &Address) {
    let creator = Address::generate(env);
    let giveaway = Giveaway {
        id: giveaway_id,
        creator,
        token: token.clone(),
        amount: 500,
        title: String::from_str(env, "Test"),
        participant_count: 0,
        end_time: env.ledger().timestamp() + 3600,
        status: GiveawayStatus::Active,
        winner_count: 1,
        winners: Vec::new(env),
        verification_type: 0,
        min_reputation: 0,
        selection_method: SelectionMethod::Random,
        claim_deadline: 0,
        claimed_count: 0,
    };
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Giveaway(giveaway_id), &giveaway);
    });
}

/// Seed a minimal open HelpRequest directly into contract storage.
fn seed_open_request(env: &Env, contract_id: &Address, request_id: u64, token: &Address) {
    let creator = Address::generate(env);
    let request = HelpRequest {
        id: request_id,
        creator,
        token: token.clone(),
        goal: 1000,
        raised_amount: 0,
        status: HelpRequestStatus::Open,
        is_verified: false,
    };
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });
}

#[test]
fn test_giveaway_suspended_at_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    // Governance and Giveaway share the same contract so storage is shared.
    let contract_id = env.register(GovernanceContract, ());
    let gov = GovernanceContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let giveaway_id: u64 = 42;
    seed_active_giveaway(&env, &contract_id, giveaway_id, &token);

    // Flag FLAG_THRESHOLD - 1 times — should still be Active.
    for _ in 0..FLAG_THRESHOLD - 1 {
        let flagger = Address::generate(&env);
        gov.flag_content(&flagger, &giveaway_id);
    }
    env.as_contract(&contract_id, || {
        let g: Giveaway = env
            .storage()
            .persistent()
            .get(&DataKey::Giveaway(giveaway_id))
            .unwrap();
        assert_eq!(g.status, GiveawayStatus::Active);
    });

    // The threshold flag suspends it.
    let last_flagger = Address::generate(&env);
    gov.flag_content(&last_flagger, &giveaway_id);

    env.as_contract(&contract_id, || {
        let g: Giveaway = env
            .storage()
            .persistent()
            .get(&DataKey::Giveaway(giveaway_id))
            .unwrap();
        assert_eq!(g.status, GiveawayStatus::Suspended);
    });
}

#[test]
fn test_help_request_suspended_at_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GovernanceContract, ());
    let gov = GovernanceContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let request_id: u64 = 7;
    seed_open_request(&env, &contract_id, request_id, &token);

    for _ in 0..FLAG_THRESHOLD {
        let flagger = Address::generate(&env);
        gov.flag_content(&flagger, &request_id);
    }

    env.as_contract(&contract_id, || {
        let r: HelpRequest = env
            .storage()
            .persistent()
            .get(&DataKey::HelpRequest(request_id))
            .unwrap();
        assert_eq!(r.status, HelpRequestStatus::Suspended);
    });
}

#[test]
fn test_content_auto_suspended_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GovernanceContract, ());
    let gov = GovernanceContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let giveaway_id: u64 = 99;
    seed_active_giveaway(&env, &contract_id, giveaway_id, &token);

    for _ in 0..FLAG_THRESHOLD {
        let flagger = Address::generate(&env);
        gov.flag_content(&flagger, &giveaway_id);
    }

    // Verify ContentAutoSuspended event was emitted with the right topic.
    let events = env.events().all();
    let expected_topics: soroban_sdk::Vec<Val> = vec![
        &env,
        Symbol::new(&env, "content_auto_suspended").into_val(&env),
        giveaway_id.into_val(&env),
    ];
    assert!(events
        .iter()
        .any(|(ec, topics, _)| { ec == contract_id && topics == expected_topics.into_val(&env) }));
}

#[test]
#[should_panic]
fn test_enter_suspended_giveaway_fails() {
    let env = Env::default();
    env.mock_all_auths();

    // Register both contracts; they share storage only when it's the same contract_id.
    // Here we use GiveawayContract for entry and seed Suspended status directly.
    let contract_id = env.register(GiveawayContract, ());
    let giveaway_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let giveaway_id: u64 = 1;
    // Seed a Suspended giveaway directly.
    let creator = Address::generate(&env);
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DataKey::Giveaway(giveaway_id),
            &Giveaway {
                id: giveaway_id,
                creator,
                token,
                amount: 500,
                title: String::from_str(&env, "Suspended"),
                participant_count: 0,
                end_time: env.ledger().timestamp() + 3600,
                status: GiveawayStatus::Suspended,
                winner_count: 1,
                winners: Vec::new(&env),
                verification_type: 0,
                min_reputation: 0,
                selection_method: SelectionMethod::Random,
                claim_deadline: 0,
                claimed_count: 0,
            },
        );
    });

    let participant = Address::generate(&env);
    // Should panic with InvalidStatus because Suspended != Active.
    giveaway_client.enter_giveaway(&participant, &giveaway_id);
}

#[test]
#[should_panic]
fn test_donate_to_suspended_request_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let aid_client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token);

    let donor = Address::generate(&env);
    token_admin_client.mint(&donor, &1000);

    let request_id: u64 = 5;
    let creator = Address::generate(&env);
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DataKey::HelpRequest(request_id),
            &HelpRequest {
                id: request_id,
                creator,
                token,
                goal: 1000,
                raised_amount: 0,
                status: HelpRequestStatus::Suspended,
                is_verified: false,
            },
        );
    });

    // Should panic with InvalidStatus.
    aid_client.donate(&donor, &request_id, &100);
}

// ── reputation tests ──────────────────────────────────────────────────────────

#[test]
fn test_reputation_starts_at_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ProfileContract, ());
    let client = ProfileContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    assert_eq!(client.get_reputation(&user), 0);
}

#[test]
fn test_reputation_increments_after_claim_prize() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Rep Test"),
        &60,
        &1,
        &None,
    );

    client.enter_giveaway(&participant, &giveaway_id);

    env.ledger().with_mut(|li| li.timestamp += 100);
    client.pick_winner(&giveaway_id);
    client.claim_prize(&giveaway_id, &participant);

    // Creator's reputation should now be 1.
    env.as_contract(&contract_id, || {
        let score: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(creator.clone()))
            .unwrap_or(0);
        assert_eq!(score, 1);
    });
}

#[test]
fn test_reputation_accumulates_across_giveaways() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant = Address::generate(&env);
    token_admin_client.mint(&creator, &2000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    // Complete two giveaways with the same creator.
    for _ in 0..2u32 {
        let giveaway_id = client.create_giveaway(
            &creator,
            &mock_token,
            &500,
            &String::from_str(&env, "Rep Test"),
            &60,
            &1,
            &None,
        );
        client.enter_giveaway(&participant, &giveaway_id);
        env.ledger().with_mut(|li| li.timestamp += 100);
        client.pick_winner(&giveaway_id);
        client.claim_prize(&giveaway_id, &participant);
    }

    env.as_contract(&contract_id, || {
        let score: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(creator.clone()))
            .unwrap_or(0);
        assert_eq!(score, 2);
    });
}

#[test]
fn test_manual_winner_selection() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant1 = Address::generate(&env);
    let participant2 = Address::generate(&env);
    let participant3 = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = client.create_giveaway_with_selection(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Manual Test"),
        &60,
        &2,
        &None,
        &SelectionMethod::Manual,
    );

    client.enter_giveaway(&participant1, &giveaway_id);
    client.enter_giveaway(&participant2, &giveaway_id);
    client.enter_giveaway(&participant3, &giveaway_id);

    env.ledger().with_mut(|li| li.timestamp += 100);

    let mut winners = Vec::new(&env);
    winners.push_back(participant2.clone());
    winners.push_back(participant3.clone());

    let winner = client.finalize_manual_winners(&creator, &giveaway_id, &winners);

    assert!(winner == participant2 || winner == participant3);

    let stored_winners: Vec<Address> = env.as_contract(&contract_id, || {
        let g: Giveaway = env
            .storage()
            .persistent()
            .get(&DataKey::Giveaway(giveaway_id))
            .unwrap();
        g.winners
    });

    assert_eq!(stored_winners.len(), 2);
    assert!(stored_winners.contains(&participant2));
    assert!(stored_winners.contains(&participant3));
}

#[test]
#[should_panic]
fn test_manual_winner_selection_fails_non_creator() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let random_user = Address::generate(&env);
    let participant = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = client.create_giveaway_with_selection(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Manual Test"),
        &60,
        &1,
        &None,
        &SelectionMethod::Manual,
    );

    client.enter_giveaway(&participant, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);

    let mut winners = Vec::new(&env);
    winners.push_back(participant.clone());
    client.finalize_manual_winners(&random_user, &giveaway_id, &winners);
}

#[test]
fn test_merit_winner_selection() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant1 = Address::generate(&env); // rep 10
    let participant2 = Address::generate(&env); // rep 50
    let participant3 = Address::generate(&env); // rep 30
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(participant1.clone()), &10u64);
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(participant2.clone()), &50u64);
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(participant3.clone()), &30u64);
    });

    let giveaway_id = client.create_giveaway_with_selection(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Merit Test"),
        &60,
        &2,
        &None,
        &SelectionMethod::Merit,
    );

    client.enter_giveaway(&participant1, &giveaway_id);
    client.enter_giveaway(&participant2, &giveaway_id);
    client.enter_giveaway(&participant3, &giveaway_id);

    env.ledger().with_mut(|li| li.timestamp += 100);

    let winner = client.finalize_merit_winners(&creator, &giveaway_id);
    assert_eq!(winner, participant2);

    let stored_winners: Vec<Address> = env.as_contract(&contract_id, || {
        let g: Giveaway = env
            .storage()
            .persistent()
            .get(&DataKey::Giveaway(giveaway_id))
            .unwrap();
        g.winners
    });

    assert_eq!(stored_winners.len(), 2);
    assert_eq!(stored_winners.get(0).unwrap(), participant2);
    assert_eq!(stored_winners.get(1).unwrap(), participant3);
}

#[test]
#[should_panic]
fn test_pick_winner_fails_on_manual_giveaway() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = client.create_giveaway_with_selection(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Test"),
        &60,
        &1,
        &None,
        &SelectionMethod::Manual,
    );

    client.enter_giveaway(&participant, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);
    client.pick_winner(&giveaway_id);
}

#[test]
fn test_admin_can_finalize_manual_winners() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    client.init(&admin, &100);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = client.create_giveaway_with_selection(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Admin Test"),
        &60,
        &1,
        &None,
        &SelectionMethod::Manual,
    );

    client.enter_giveaway(&participant, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);

    let mut winners = Vec::new(&env);
    winners.push_back(participant.clone());

    client.finalize_manual_winners(&admin, &giveaway_id, &winners);
}

#[test]
fn test_transfer_admin_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let current_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::Admin, &current_admin);
    });

    contract_client.transfer_admin(&current_admin, &new_admin);

    // events().all() only returns events from the last contract invocation
    let events = env.events().all();
    let expected_topics: soroban_sdk::Vec<Val> = vec![
        &env,
        symbol_short!("admin").into_val(&env),
        symbol_short!("transfer").into_val(&env),
        current_admin.into_val(&env),
    ];
    assert!(
        events.iter().any(|(event_contract, topics, data)| {
            if event_contract != contract_id || topics != expected_topics.into_val(&env) {
                return false;
            }
            let data_vec: soroban_sdk::Vec<Val> = soroban_sdk::Vec::from_val(&env, &data);
            let next = Address::from_val(&env, &data_vec.get(0).unwrap());
            next == new_admin
        }),
        "AdminTransferred event was not emitted with the expected addresses"
    );

    env.as_contract(&contract_id, || {
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert_eq!(stored_admin, new_admin);
    });
}

#[test]
#[should_panic]
fn test_transfer_admin_fails_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let current_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // DO NOT initialize admin - should panic
    contract_client.transfer_admin(&current_admin, &new_admin);
}

#[test]
#[should_panic]
fn test_transfer_admin_fails_wrong_current_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let current_admin = Address::generate(&env);
    let impostor = Address::generate(&env);
    let new_admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::Admin, &current_admin);
    });

    // Impostor address does not match stored admin - should panic
    contract_client.transfer_admin(&impostor, &new_admin);
}

#[test]
fn test_new_admin_can_perform_gated_actions_after_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let current_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let token = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::Admin, &current_admin);
    });

    contract_client.transfer_admin(&current_admin, &new_admin);

    // New admin can perform gated actions
    contract_client.add_token(&token);

    env.as_contract(&contract_id, || {
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        assert_eq!(stored_admin, new_admin);

        let is_allowed: bool = env
            .storage()
            .instance()
            .get(&DataKey::AllowedToken(token.clone()))
            .unwrap_or(false);
        assert!(is_allowed);
    });
}

#[test]
fn test_remove_token_revokes_whitelist() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Admin, &admin);
    });

    contract_client.add_token(&token);
    contract_client.remove_token(&token);

    env.as_contract(&contract_id, || {
        let is_allowed: bool = env
            .storage()
            .instance()
            .get(&DataKey::AllowedToken(token.clone()))
            .unwrap_or(false);
        assert!(!is_allowed);
    });
}

#[test]
#[should_panic]
fn test_remove_token_fails_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AdminContract, ());
    let contract_client = AdminContractClient::new(&env, &contract_id);

    let token = Address::generate(&env);

    // DO NOT initialize admin - should panic
    contract_client.remove_token(&token);
}

#[test]
fn test_create_giveaway_succeeds_before_and_fails_after_delist() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    token_admin_client.mint(&creator, &2000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    // Creation succeeds while token is allowlisted
    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Before Delist"),
        &60,
        &1,
        &None,
    );
    assert_eq!(giveaway_id, 1);

    // Delist token (same persistence as AdminContract::remove_token)
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &false);
    });

    // Creation with delisted token must fail
    let result = contract_client.try_create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "After Delist"),
        &60,
        &1,
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_existing_giveaway_continues_after_token_delist() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Funded Before Delist"),
        &60,
        &1,
        &None,
    );

    // Delist after funding — existing giveaway lifecycle remains supported
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &false);
    });

    contract_client.enter_giveaway(&participant, &giveaway_id);

    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });

    let winner = contract_client.pick_winner(&giveaway_id);
    assert_eq!(winner, participant);
}

// ── claim timeout & recovery tests ────────────────────────────────────────

#[test]
#[should_panic]
fn test_claim_prize_after_expiry_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let winner = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Expiry Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&winner, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);
    contract_client.pick_winner(&giveaway_id);

    // Advance past the claim window (7 days).
    env.ledger()
        .with_mut(|li| li.timestamp += 7 * 24 * 60 * 60 + 1);

    contract_client.claim_prize(&giveaway_id, &winner);
}

#[test]
#[should_panic]
fn test_claim_prize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let winner = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Double Claim Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&winner, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);
    contract_client.pick_winner(&giveaway_id);

    contract_client.claim_prize(&giveaway_id, &winner);
    contract_client.claim_prize(&giveaway_id, &winner);
}

#[test]
#[should_panic]
fn test_claim_prize_by_non_winner_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant1 = Address::generate(&env);
    let participant2 = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Non Winner Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&participant1, &giveaway_id);
    contract_client.enter_giveaway(&participant2, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);
    let winner = contract_client.pick_winner(&giveaway_id);
    let non_winner = if winner == participant1 {
        participant2
    } else {
        participant1
    };

    contract_client.claim_prize(&giveaway_id, &non_winner);
}

#[test]
#[should_panic]
fn test_recover_before_expiry_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let winner = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Early Recovery Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&winner, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);
    contract_client.pick_winner(&giveaway_id);

    contract_client.recover_unclaimed_prize(&giveaway_id, &creator);
}

#[test]
fn test_recover_after_expiry_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let winner = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Recovery Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&winner, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);
    contract_client.pick_winner(&giveaway_id);

    // Creator's balance after funding the giveaway.
    assert_eq!(token_client.balance(&creator), 500);

    env.ledger()
        .with_mut(|li| li.timestamp += 7 * 24 * 60 * 60 + 1);
    contract_client.recover_unclaimed_prize(&giveaway_id, &creator);

    // The full unclaimed amount (no fee — it was never claimed) returns to the creator.
    assert_eq!(token_client.balance(&creator), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(token_client.balance(&winner), 0);

    env.as_contract(&contract_id, || {
        let giveaway: Giveaway = env
            .storage()
            .persistent()
            .get(&DataKey::Giveaway(giveaway_id))
            .unwrap();
        assert_eq!(giveaway.status, GiveawayStatus::Completed);
    });
}

#[test]
#[should_panic]
fn test_recover_by_unauthorized_caller_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let winner = Address::generate(&env);
    let stranger = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &500,
        &String::from_str(&env, "Unauthorized Recovery Test"),
        &60,
        &1,
        &None,
    );

    contract_client.enter_giveaway(&winner, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);
    contract_client.pick_winner(&giveaway_id);
    env.ledger()
        .with_mut(|li| li.timestamp += 7 * 24 * 60 * 60 + 1);

    contract_client.recover_unclaimed_prize(&giveaway_id, &stranger);
}

#[test]
fn test_partial_claim_recovery() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(GiveawayContract, ());
    let contract_client = GiveawayContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let mock_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &mock_token);
    let token_admin_client = token::StellarAssetClient::new(&env, &mock_token);

    let creator = Address::generate(&env);
    let participant1 = Address::generate(&env);
    let participant2 = Address::generate(&env);
    let participant3 = Address::generate(&env);
    token_admin_client.mint(&creator, &1000);

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(mock_token.clone()), &true);
    });

    let giveaway_id = contract_client.create_giveaway(
        &creator,
        &mock_token,
        &400,
        &String::from_str(&env, "Partial Claim Recovery Test"),
        &60,
        &2,
        &None,
    );

    contract_client.enter_giveaway(&participant1, &giveaway_id);
    contract_client.enter_giveaway(&participant2, &giveaway_id);
    contract_client.enter_giveaway(&participant3, &giveaway_id);
    env.ledger().with_mut(|li| li.timestamp += 100);
    contract_client.pick_winner(&giveaway_id);

    let winners: Vec<Address> = env.as_contract(&contract_id, || {
        let giveaway: Giveaway = env
            .storage()
            .persistent()
            .get(&DataKey::Giveaway(giveaway_id))
            .unwrap();
        giveaway.winners.clone()
    });
    let claimer = winners.get(0).unwrap();
    let ghost = winners.get(1).unwrap();

    // Only one of the two winners claims before the deadline.
    contract_client.claim_prize(&giveaway_id, &claimer);
    let claimer_balance_after_claim = token_client.balance(&claimer);
    assert!(claimer_balance_after_claim > 0);

    env.ledger()
        .with_mut(|li| li.timestamp += 7 * 24 * 60 * 60 + 1);
    contract_client.recover_unclaimed_prize(&giveaway_id, &creator);

    // The claimer's funds are untouched; only the ghost winner's share is recovered.
    // The contract keeps the 2-token fee collected from the claimer's claim
    // (1% of their 200-token gross share) until withdraw_fees is called.
    assert_eq!(token_client.balance(&claimer), claimer_balance_after_claim);
    assert_eq!(token_client.balance(&ghost), 0);
    assert_eq!(token_client.balance(&contract_id), 2);

    env.as_contract(&contract_id, || {
        let giveaway: Giveaway = env
            .storage()
            .persistent()
            .get(&DataKey::Giveaway(giveaway_id))
            .unwrap();
        assert_eq!(giveaway.status, GiveawayStatus::Completed);
    });
}

// ── mutual aid creator claim tests ────────────────────────────────────────

/// A `panic_with_error!` reaches a `try_*` client as an encoded contract error,
/// so expected failures are compared against the encoded form of the variant.
fn contract_error(error: Error) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(error as u32)
}

/// Register the mutual-aid contract against a real token, then post and fully
/// fund a help request. Returns `(contract_id, client, token, creator, request_id)`.
fn setup_funded_request(
    env: &Env,
) -> (Address, MutualAidContractClient<'_>, Address, Address, u64) {
    let contract_id = env.register(MutualAidContract, ());
    let client = MutualAidContractClient::new(env, &contract_id);

    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let creator = Address::generate(env);
    let donor = Address::generate(env);
    token::StellarAssetClient::new(env, &token).mint(&donor, &1000);

    let request_id = client.post_help_request(&creator, &1, &1000, &token);
    client.donate(&donor, &request_id, &1000);

    (contract_id, client, token, creator, request_id)
}

/// Overwrite a help request's raised amount and status, to reach states that no
/// public entry point produces yet (e.g. a dispute resolved in either direction).
fn force_request_state(
    env: &Env,
    contract_id: &Address,
    request_id: u64,
    token: &Address,
    creator: &Address,
    raised_amount: i128,
    status: HelpRequestStatus,
) {
    let request = HelpRequest {
        id: request_id,
        creator: creator.clone(),
        token: token.clone(),
        goal: 1000,
        raised_amount,
        status,
        is_verified: false,
    };
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::HelpRequest(request_id), &request);
    });
}

#[test]
fn test_creator_claims_fully_funded_request() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, token, creator, request_id) = setup_funded_request(&env);
    let token_client = token::Client::new(&env, &token);

    assert_eq!(token_client.balance(&contract_id), 1000);
    assert_eq!(token_client.balance(&creator), 0);

    client.claim_help_request_funds(&creator, &request_id);

    // The whole escrow reaches the creator: mutual aid takes no protocol fee.
    assert_eq!(token_client.balance(&creator), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);

    let request = client.get_request(&request_id).unwrap();
    assert_eq!(request.status, HelpRequestStatus::Closed);
    assert_eq!(request.raised_amount, 1000);

    env.as_contract(&contract_id, || {
        let claimed: bool = env
            .storage()
            .persistent()
            .get(&DataKey::HelpRequestClaimed(request_id))
            .unwrap();
        assert!(claimed);
    });
}

#[test]
fn test_claim_emits_funds_claimed_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _token, creator, request_id) = setup_funded_request(&env);

    client.claim_help_request_funds(&creator, &request_id);

    let events = env.events().all();
    let expected_topics: soroban_sdk::Vec<Val> = vec![
        &env,
        symbol_short!("aid").into_val(&env),
        symbol_short!("claim").into_val(&env),
        request_id.into_val(&env),
    ];
    assert!(
        events.iter().any(|(event_contract, topics, data)| {
            if event_contract != contract_id || topics != expected_topics.into_val(&env) {
                return false;
            }
            let data_vec: soroban_sdk::Vec<Val> = soroban_sdk::Vec::from_val(&env, &data);
            let actual_creator = Address::from_val(&env, &data_vec.get(0).unwrap());
            let actual_amount = i128::from_val(&env, &data_vec.get(1).unwrap());
            actual_creator == creator && actual_amount == 1000
        }),
        "HelpRequestFundsClaimed event did not contain the creator and claimed amount"
    );
}

#[test]
fn test_creator_cannot_claim_twice() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, token, creator, request_id) = setup_funded_request(&env);
    let token_client = token::Client::new(&env, &token);

    client.claim_help_request_funds(&creator, &request_id);

    // The payout closes the request, so a repeat withdrawal has no release state.
    assert_eq!(
        client.try_claim_help_request_funds(&creator, &request_id),
        Err(Ok(contract_error(Error::InvalidStatus)))
    );
    assert_eq!(token_client.balance(&creator), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_claim_record_blocks_second_payout() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, token, creator, request_id) = setup_funded_request(&env);
    let token_client = token::Client::new(&env, &token);

    client.claim_help_request_funds(&creator, &request_id);

    // Force the request back into a release state: the one-shot claim record,
    // not just the Closed status, is what makes a second payout impossible.
    force_request_state(
        &env,
        &contract_id,
        request_id,
        &token,
        &creator,
        1000,
        HelpRequestStatus::FullyFunded,
    );

    assert_eq!(
        client.try_claim_help_request_funds(&creator, &request_id),
        Err(Ok(contract_error(Error::AlreadyClaimed)))
    );
    assert_eq!(token_client.balance(&creator), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_non_creator_cannot_claim_request_funds() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, token, creator, request_id) = setup_funded_request(&env);
    let token_client = token::Client::new(&env, &token);
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_claim_help_request_funds(&stranger, &request_id),
        Err(Ok(contract_error(Error::NotCreator)))
    );
    assert_eq!(token_client.balance(&stranger), 0);
    assert_eq!(token_client.balance(&creator), 0);
    assert_eq!(token_client.balance(&contract_id), 1000);
}

#[test]
fn test_claim_before_fully_funded_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::Client::new(&env, &token);

    let creator = Address::generate(&env);
    let donor = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token).mint(&donor, &400);

    let request_id = client.post_help_request(&creator, &1, &1000, &token);
    client.donate(&donor, &request_id, &400);

    assert_eq!(
        client.try_claim_help_request_funds(&creator, &request_id),
        Err(Ok(contract_error(Error::InvalidStatus)))
    );
    assert_eq!(token_client.balance(&creator), 0);
    assert_eq!(token_client.balance(&contract_id), 400);
}

#[test]
fn test_claim_cancelled_request_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let client = MutualAidContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let creator = Address::generate(&env);
    let request_id: u64 = 7;
    force_request_state(
        &env,
        &contract_id,
        request_id,
        &token,
        &creator,
        500,
        HelpRequestStatus::Cancelled,
    );

    // Cancelled escrow belongs to the donors, who reclaim it via claim_refund.
    assert_eq!(
        client.try_claim_help_request_funds(&creator, &request_id),
        Err(Ok(contract_error(Error::InvalidStatus)))
    );
}

#[test]
fn test_claim_missing_request_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MutualAidContract, ());
    let client = MutualAidContractClient::new(&env, &contract_id);
    let creator = Address::generate(&env);

    assert_eq!(
        client.try_claim_help_request_funds(&creator, &404),
        Err(Ok(contract_error(Error::HelpRequestNotFound)))
    );
}

#[test]
fn test_claim_after_dispute_resolved_release() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, token, creator, request_id) = setup_funded_request(&env);
    let token_client = token::Client::new(&env, &token);

    // A dispute settled in the creator's favour is the other documented release
    // state, so it must allow a withdrawal too.
    force_request_state(
        &env,
        &contract_id,
        request_id,
        &token,
        &creator,
        1000,
        HelpRequestStatus::ResolvedRelease,
    );

    client.claim_help_request_funds(&creator, &request_id);

    assert_eq!(token_client.balance(&creator), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(
        client.get_request(&request_id).unwrap().status,
        HelpRequestStatus::Closed
    );
}

#[test]
fn test_donate_to_closed_request_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, token, creator, request_id) = setup_funded_request(&env);
    let token_client = token::Client::new(&env, &token);

    client.claim_help_request_funds(&creator, &request_id);

    let late_donor = Address::generate(&env);
    token::StellarAssetClient::new(&env, &token).mint(&late_donor, &100);

    // A donation to a paid out request could never be released or refunded.
    assert_eq!(
        client.try_donate(&late_donor, &request_id, &100),
        Err(Ok(contract_error(Error::InvalidStatus)))
    );
    assert_eq!(token_client.balance(&late_donor), 100);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_no_refund_path_after_creator_claim() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, token, creator, request_id) = setup_funded_request(&env);
    let token_client = token::Client::new(&env, &token);

    client.claim_help_request_funds(&creator, &request_id);

    // Refunds need a Cancelled request and cancelling needs an Open one, so a
    // paid out request cannot be turned back into refundable escrow.
    assert_eq!(
        client.try_cancel_request(&creator, &request_id),
        Err(Ok(contract_error(Error::InvalidStatus)))
    );
    assert_eq!(token_client.balance(&creator), 1000);
    assert_eq!(token_client.balance(&contract_id), 0);
}
