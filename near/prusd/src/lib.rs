//! Pro Rata Dollar (PRUSD) on NEAR. No owner, no pause, no upgrade key.
//!
//! D1 atomic par:  send x reserve tokens with `ft_transfer_call(msg = "mint")`, receive x PRUSD.
//!                 `redeem(x)` burns x PRUSD and pays x * min(1, B), B = reserve / supply.
//! D2 pro rata:    when B < 1 every redemption pays the same fraction, so redeeming first
//!                 gains nothing over redeeming last.
//! D3 buffer:      `ft_transfer_call(msg = "fund")` adds reserve without minting. No method
//!                 pays the buffer out to anyone.
//!
//! Invariants, by cases over every method that writes `reserve` or `supply`:
//!  I1  if reserve >= supply before a call, then reserve >= supply after it.
//!      mint: both += x.  fund: reserve += y.  redeem: both -= x.  failed payout: both += x.
//!  I2  reserve - supply never decreases while reserve >= supply (same four cases).
//!  I3  under redeem with reserve < supply, out = floor(x * reserve / supply) <= x * B,
//!      so (reserve - out) / (supply - x) >= B: the backing ratio does not fall.
//! `reserve` is this contract's own ledger of tokens received through `ft_on_transfer`, so a
//! plain transfer to the contract cannot move it.
use near_sdk::json_types::U128;
use near_sdk::serde_json::{json, Value};
use near_sdk::store::LookupMap;
use near_sdk::{env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError, PromiseOrValue};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Prusd {
    reserve_token: AccountId,
    reserve: u128,
    supply: u128,
    balances: LookupMap<AccountId, u128>,
}

#[near]
impl Prusd {
    #[init]
    pub fn new(reserve_token: AccountId) -> Self {
        Self { reserve_token, reserve: 0, supply: 0, balances: LookupMap::new(b"b") }
    }

    fn bal(&self, a: &AccountId) -> u128 {
        self.balances.get(a).copied().unwrap_or(0)
    }

    /// Entry point for mint and fund: called by the reserve token after it has moved `amount` here.
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128> {
        require!(env::predecessor_account_id() == self.reserve_token, "reserve token only");
        let x = amount.0;
        self.reserve += x;
        if msg == "fund" {
            env::log_str(&format!("fund {} {}", sender_id, x));
        } else {
            self.supply += x;
            let b = self.bal(&sender_id) + x;
            self.balances.insert(sender_id.clone(), b);
            env::log_str(&format!("mint {} {}", sender_id, x));
        }
        PromiseOrValue::Value(U128(0))
    }

    /// Burn `coins`, receive coins * min(1, B) of the reserve token.
    #[payable]
    pub fn redeem(&mut self, coins: U128) -> Promise {
        require!(env::attached_deposit() == NearToken::from_yoctonear(1), "attach 1 yoctoNEAR");
        let who = env::predecessor_account_id();
        let x = coins.0;
        let b = self.bal(&who);
        require!(x > 0 && b >= x, "balance");
        let out = if self.reserve >= self.supply { x } else { x * self.reserve / self.supply };
        self.balances.insert(who.clone(), b - x);
        self.supply -= x;
        self.reserve -= out;
        Promise::new(self.reserve_token.clone())
            .function_call(
                "ft_transfer".to_string(),
                json!({ "receiver_id": who, "amount": U128(out) }).to_string().into_bytes(),
                NearToken::from_yoctonear(1),
                Gas::from_tgas(15),
            )
            .then(Promise::new(env::current_account_id()).function_call(
                "on_redeem".to_string(),
                json!({ "account": who, "coins": coins, "out": U128(out) }).to_string().into_bytes(),
                NearToken::from_yoctonear(0),
                Gas::from_tgas(10),
            ))
    }

    /// If the payout did not go through, the burn is undone.
    #[private]
    pub fn on_redeem(&mut self, account: AccountId, coins: U128, out: U128, #[callback_result] paid: Result<(), PromiseError>) -> U128 {
        if paid.is_ok() {
            env::log_str(&format!("redeem {} {} {}", account, coins.0, out.0));
            return out;
        }
        self.reserve += out.0;
        self.supply += coins.0;
        let b = self.bal(&account) + coins.0;
        self.balances.insert(account, b);
        U128(0)
    }

    #[payable]
    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        require!(env::attached_deposit() == NearToken::from_yoctonear(1), "attach 1 yoctoNEAR");
        let _ = memo;
        let from = env::predecessor_account_id();
        let b = self.bal(&from);
        require!(amount.0 > 0 && b >= amount.0 && from != receiver_id, "transfer");
        self.balances.insert(from, b - amount.0);
        let r = self.bal(&receiver_id) + amount.0;
        self.balances.insert(receiver_id, r);
    }

    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        U128(self.bal(&account_id))
    }

    pub fn ft_total_supply(&self) -> U128 {
        U128(self.supply)
    }

    pub fn ft_metadata(&self) -> Value {
        json!({ "spec": "ft-1.0.0", "name": "Pro Rata Dollar", "symbol": "PRUSD", "decimals": 6 })
    }

    pub fn reserve(&self) -> U128 {
        U128(self.reserve)
    }

    pub fn reserve_token(&self) -> AccountId {
        self.reserve_token.clone()
    }

    /// Backing ratio B in parts per million.
    pub fn backing_ppm(&self) -> U128 {
        U128(if self.supply == 0 { 1_000_000 } else { self.reserve * 1_000_000 / self.supply })
    }

    /// Buffer B - 1 in basis points, 0 when the reserve does not exceed the supply.
    pub fn buffer_bps(&self) -> U128 {
        U128(if self.supply == 0 || self.reserve <= self.supply { 0 } else { (self.reserve - self.supply) * 10_000 / self.supply })
    }
}
