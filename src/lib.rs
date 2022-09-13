/*!
Fungible Token implementation with JSON serialization.
NOTES:
  - The maximum balance value is limited by U128 (2**128 - 1).
  - JSON calls should pass U128 as a base-10 string. E.g. "100".
  - The contract optimizes the inner trie structure by hashing account IDs. It will prevent some
    abuse of deep tries. Shouldn't be an issue, once NEAR clients implement full hashing of keys.
  - The contract tracks the change in storage before and after the call. If the storage increases,
    the contract requires the caller of the contract to attach enough deposit to the function call
    to cover the storage cost.
    This is done to prevent a denial of service attack on the contract by taking all available storage.
    If the storage decreases, the contract will issue a refund for the cost of the released storage.
    The unused tokens from the attached deposit are also refunded, so it's safe to
    attach more deposit than required.
  - To prevent the deployed contract from being modified or deleted, it should not have any access
    keys on its account.
*/
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider,
};
// use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedMap};
use near_sdk::json_types::U128;
use near_sdk::{env, near_bindgen, AccountId, Balance, PanicOnDefault};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    metadata: LazyOption<FungibleTokenMetadata>,
    fundAccounts: UnorderedMap<String, Balance>,
    accounts: UnorderedMap<AccountId, Balance>,
    canMint: bool,
    canBurn: bool,
    ownerId: AccountId,
    glueId: AccountId,
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 288 288'%3E%3Cg id='l' data-name='l'%3E%3Cpath d='M187.58,79.81l-30.1,44.69a3.2,3.2,0,0,0,4.75,4.2L191.86,103a1.2,1.2,0,0,1,2,.91v80.46a1.2,1.2,0,0,1-2.12.77L102.18,77.93A15.35,15.35,0,0,0,90.47,72.5H87.34A15.34,15.34,0,0,0,72,87.84V201.16A15.34,15.34,0,0,0,87.34,216.5h0a15.35,15.35,0,0,0,13.08-7.31l30.1-44.69a3.2,3.2,0,0,0-4.75-4.2L96.14,186a1.2,1.2,0,0,1-2-.91V104.61a1.2,1.2,0,0,1,2.12-.77l89.55,107.23a15.35,15.35,0,0,0,11.71,5.43h3.13A15.34,15.34,0,0,0,216,201.16V87.84A15.34,15.34,0,0,0,200.66,72.5h0A15.35,15.35,0,0,0,187.58,79.81Z'/%3E%3C/g%3E%3C/svg%3E";

#[near_bindgen]
impl Contract {
    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// default metadata (for example purposes only).


    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// the given fungible token metadata.
    #[init]
    pub fn new(
        total_supply: U128,
        metadata: FungibleTokenMetadata,
        can_mint: bool,
        can_burn: bool,
        glue_id: AccountId,
    ) -> Self {
        let owner_id = env::signer_account_id();
        assert!(!env::state_exists(), "Already initialized");
        metadata.assert_valid();
        let mut this = Self {
            metadata: LazyOption::new(b"m".to_vec(), Some(&metadata)),
            accounts: UnorderedMap::new(b"a".to_vec()),
            fundAccounts: UnorderedMap::new(b"f".to_vec()),
            canBurn: can_burn,
            canMint: can_mint,
            ownerId: owner_id.clone(),
            glueId: glue_id,
        };
        this.accounts.insert(&owner_id, &u128::from(total_supply));
        FtMint {
            owner_id: &owner_id,
            amount: &total_supply,
            memo: Some("Initial tokens supply is minted"),
        }
        .emit();
        this
    }

    fn internal_deposit(&mut self, account: &AccountId, amount: u128) {
        let balance = self.accounts.get(account).unwrap_or(0);
        self.accounts.insert(&account, &(balance + amount));
        
    }
    fn internal_withdraw(&mut self, account: &AccountId, amount: u128) {
        let balance = self.accounts.get(account).unwrap_or(0);
        assert!(balance >= amount);
        self.accounts.insert(&account, &(balance - amount));
    }

    pub fn burnToken(&mut self, amount: U128) {
        assert!(self.canBurn);
        assert!(env::signer_account_id() == self.ownerId);
        self.internal_withdraw(&self.ownerId.clone(), amount.into());
    }
    pub fn mintToken(&mut self, amount: U128) {
        assert!(self.canMint);
        assert!(env::signer_account_id() == self.ownerId);
        self.internal_deposit(&self.ownerId.clone(), amount.into());
    }

    // pub fn sendToken()
    // // @call({})
    // // sendToken({ walletAddress, amount }) {
    // //     assert(near.signerAccountId() == this.owner, "Only owner can call")
    // //     this.internalTransfer(this.owner, walletAddress, amount, "")
    // // }

    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128) {
        let sender = env::signer_account_id();
        self.internal_withdraw(&sender, amount.into());
        self.internal_deposit(&receiver_id, amount.into());
        FtTransfer {
            old_owner_id: &sender,
            new_owner_id: &receiver_id,
            amount: &amount,
            memo: Some("transfered"),
        }
        .emit();
    }

    // add to users tokens to fund map
    pub fn sendToFund(&mut self, id: String, amount: U128) {
        let signer = env::signer_account_id();
        assert!(signer == self.ownerId || signer == self.glueId);
        self.internal_withdraw(&self.ownerId.clone(), amount.into());
        let balance: u128 = self.fundAccounts.get(&id).unwrap_or(0);
        let new_balance: u128 = balance + u128::from(amount);
        self.fundAccounts.insert(&id, &new_balance);
    }

    // claims tokens from fund to users web3 account
    pub fn sendFromFund(&mut self, id: String, walletAddress: AccountId, amount: U128) {
        let signer = env::signer_account_id();
        assert!(signer == self.ownerId || signer == self.glueId);
        let balance = self.fundAccounts.get(&id).unwrap_or(0);
        let amountInt = u128::from(amount);
        assert!(balance >= amountInt);
        self.fundAccounts.insert(&id, &(balance - amountInt));
        self.internal_deposit(&walletAddress, amountInt);
        FtTransfer {
            old_owner_id: &self.ownerId,
            new_owner_id: &walletAddress,
            amount: &amount,
            memo: Some("transfered"),
        }
        .emit();
    }

    pub fn changeOwner(&mut self, address: AccountId) {
        assert!(env::signer_account_id() == self.ownerId);
        self.ownerId = address;
    }

    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        let balance = self.accounts.get(&account_id).unwrap_or(0);
        U128::from(balance)
    }

    pub fn ft_fund_balance_of(&self, account_id: String) -> U128 {
        let balance = self.fundAccounts.get(&account_id).unwrap_or(0);
        U128::from(balance)
    }

    pub fn total_users(&self) -> u64 {
        self.accounts.len()
    }

    pub fn total_fund_users(&self) -> u64 {
        self.fundAccounts.len()
    }

    pub fn list_balances(&self, start_index: usize, limit: usize) -> Vec<(AccountId, U128)>{
        self.accounts
            .iter()
            .skip(start_index)
            .take(limit)
            .map(|(account_id, balance)| (account_id, U128::from(balance)))
            .collect()
    }

    pub fn list_fund_balances(&self, start_index: usize, limit: usize) -> Vec<(String, U128)>{
        self.fundAccounts
            .iter()
            .skip(start_index)
            .take(limit)
            .map(|(account_id, balance)| (account_id, U128::from(balance)))
            .collect()
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}


//ß Standard for nep141 (Fungible Token) events.
//ß
//ß These events will be picked up by the NEAR indexer.
//ß
//ß <https://github.com/near/NEPs/blob/master/specs/Standards/FungibleToken/Event.md>
//ß
//ß This is an extension of the events format (nep-297):
//ß <https://github.com/near/NEPs/blob/master/specs/Standards/EventsFormat.md>
//ß
//ß The three events in this standard are [`FtMint`], [`FtTransfer`], and [`FtBurn`].
//ß
//ß These events can be logged by calling `.emit()` on them if a single event, or calling
//ß [`FtMint::emit_many`], [`FtTransfer::emit_many`],
//ß or [`FtBurn::emit_many`] respectively.

use near_sdk::serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(tag = "standard")]
#[must_use = "don't forget to `.emit()` this event"]
#[serde(rename_all = "snake_case")]
pub(crate) enum NearEvent<'a> {
    Nep141(Nep141Event<'a>),
}

impl<'a> NearEvent<'a> {
    fn to_json_string(&self) -> String {
        // Events cannot fail to serialize so fine to panic on error
        #[allow(clippy::redundant_closure)]
        serde_json::to_string(self).ok().unwrap_or_else(|| env::abort())
    }

    fn to_json_event_string(&self) -> String {
        format!("EVENT_JSON:{}", self.to_json_string())
    }

    /// Logs the event to the host. This is required to ensure that the event is triggered
    /// and to consume the event.
    pub(crate) fn emit(self) {
        near_sdk::env::log_str(&self.to_json_event_string());
    }
}


/// Data to log for an FT mint event. To log this event, call [`.emit()`](FtMint::emit).
#[must_use]
#[derive(Serialize, Debug, Clone)]
pub struct FtMint<'a> {
    pub owner_id: &'a AccountId,
    pub amount: &'a U128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<&'a str>,
}

impl FtMint<'_> {
    /// Logs the event to the host. This is required to ensure that the event is triggered
    /// and to consume the event.
    pub fn emit(self) {
        Self::emit_many(&[self])
    }

    /// Emits an FT mint event, through [`env::log_str`](near_sdk::env::log_str),
    /// where each [`FtMint`] represents the data of each mint.
    pub fn emit_many(data: &[FtMint<'_>]) {
        new_141_v1(Nep141EventKind::FtMint(data)).emit()
    }
}

/// Data to log for an FT transfer event. To log this event,
/// call [`.emit()`](FtTransfer::emit).
#[must_use]
#[derive(Serialize, Debug, Clone)]
pub struct FtTransfer<'a> {
    pub old_owner_id: &'a AccountId,
    pub new_owner_id: &'a AccountId,
    pub amount: &'a U128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<&'a str>,
}

impl FtTransfer<'_> {
    /// Logs the event to the host. This is required to ensure that the event is triggered
    /// and to consume the event.
    pub fn emit(self) {
        Self::emit_many(&[self])
    }

    /// Emits an FT transfer event, through [`env::log_str`](near_sdk::env::log_str),
    /// where each [`FtTransfer`] represents the data of each transfer.
    pub fn emit_many(data: &[FtTransfer<'_>]) {
        new_141_v1(Nep141EventKind::FtTransfer(data)).emit()
    }
}

#[derive(Serialize, Debug)]
pub(crate) struct Nep141Event<'a> {
    version: &'static str,
    #[serde(flatten)]
    event_kind: Nep141EventKind<'a>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
enum Nep141EventKind<'a> {
    FtMint(&'a [FtMint<'a>]),
    FtTransfer(&'a [FtTransfer<'a>]),
}

fn new_141<'a>(version: &'static str, event_kind: Nep141EventKind<'a>) -> NearEvent<'a> {
    NearEvent::Nep141(Nep141Event { version, event_kind })
}

fn new_141_v1(event_kind: Nep141EventKind) -> NearEvent {
    new_141("1.0.0", event_kind)
}
