//! Pro Rata Dollar (PRUSD) on NEAR: a dollar coin backed by several dollar stablecoins that
//! stays redeemable at par if any one of them fails, with no oracle, no owner, no pause and
//! no upgrade key. The rules and their proofs are in core.rs; this file moves the tokens.
//!
//! Flow: send reserve tokens here with `ft_transfer_call` (they become your credit), then
//! `mint` or `fund`. `redeem`, `withdraw_junior` and `withdraw` pay out by `ft_transfer`; a
//! payout that fails is returned to your credit.
mod core;
use crate::core::Core;
use near_sdk::json_types::U128;
use near_sdk::serde_json::{json, Value};
use near_sdk::store::LookupMap;
use near_sdk::{env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError, PromiseOrValue};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Prusd {
    legs: Vec<AccountId>,
    core: Core,
    balances: LookupMap<AccountId, u128>,
    junior: LookupMap<AccountId, u128>,
    credits: LookupMap<(AccountId, u8), u128>,
}

fn ok<T>(r: Result<T, &'static str>) -> T {
    r.unwrap_or_else(|e| env::panic_str(e))
}

fn one_yocto() {
    require!(env::attached_deposit() == NearToken::from_yoctonear(1), "attach 1 yoctoNEAR");
}

#[near]
impl Prusd {
    #[init]
    pub fn new(legs: Vec<AccountId>) -> Self {
        require!((2..=4).contains(&legs.len()), "2 to 4 legs");
        for (i, a) in legs.iter().enumerate() {
            require!(!legs[..i].contains(a), "legs must differ");
        }
        Self { core: Core::new(legs.len()), legs, balances: LookupMap::new(b"b"), junior: LookupMap::new(b"j"), credits: LookupMap::new(b"c") }
    }

    fn credit(&self, a: &AccountId, k: u8) -> u128 {
        self.credits.get(&(a.clone(), k)).copied().unwrap_or(0)
    }
    fn add_credit(&mut self, a: &AccountId, k: u8, x: u128) {
        let v = self.credit(a, k) + x;
        self.credits.insert((a.clone(), k), v);
    }
    fn take_credit(&mut self, a: &AccountId, per: u128) {
        for k in 0..self.legs.len() as u8 {
            let v = self.credit(a, k);
            require!(v >= per, "not enough credit in every leg: deposit each reserve token first");
            self.credits.insert((a.clone(), k), v - per);
        }
    }
    fn pay(&self, to: &AccountId, k: usize, amount: u128) {
        if amount == 0 {
            return;
        }
        let _ = Promise::new(self.legs[k].clone())
            .function_call("ft_transfer".to_string(), json!({ "receiver_id": to, "amount": U128(amount) }).to_string().into_bytes(), NearToken::from_yoctonear(1), Gas::from_tgas(12))
            .then(Promise::new(env::current_account_id()).function_call(
                "on_paid".to_string(),
                json!({ "account": to, "leg": k as u8, "amount": U128(amount) }).to_string().into_bytes(),
                NearToken::from_yoctonear(0),
                Gas::from_tgas(6),
            ));
    }

    /// Reserve tokens arriving by `ft_transfer_call` become the sender's credit.
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128> {
        let _ = msg;
        let who = env::predecessor_account_id();
        let k = self.legs.iter().position(|a| *a == who).unwrap_or_else(|| env::panic_str("not a reserve token"));
        self.add_credit(&sender_id, k as u8, amount.0);
        PromiseOrValue::Value(U128(0))
    }

    /// Mint `coins` PRUSD from credit: coins/n plus 0.10% from every leg.
    pub fn mint(&mut self, coins: U128) -> U128 {
        let who = env::predecessor_account_id();
        let per = ok(self.core.mint(coins.0));
        self.take_credit(&who, per);
        let b = self.balances.get(&who).copied().unwrap_or(0) + coins.0;
        self.balances.insert(who, b);
        U128(per)
    }

    /// Add `amount` to the buffer from credit, in equal parts, and receive buffer shares.
    pub fn fund(&mut self, amount: U128) -> U128 {
        let who = env::predecessor_account_id();
        let (per, shares) = ok(self.core.fund(amount.0));
        self.take_credit(&who, per);
        let j = self.junior.get(&who).copied().unwrap_or(0) + shares;
        self.junior.insert(who, j);
        U128(shares)
    }

    /// Burn `coins` and take `take[k]` from leg k. Use `quote_redeem` for a valid `take`.
    #[payable]
    pub fn redeem(&mut self, coins: U128, take: Vec<U128>) {
        one_yocto();
        let who = env::predecessor_account_id();
        let b = self.balances.get(&who).copied().unwrap_or(0);
        require!(b >= coins.0, "balance");
        let take: Vec<u128> = take.iter().map(|t| t.0).collect();
        ok(self.core.redeem(coins.0, &take));
        self.balances.insert(who.clone(), b - coins.0);
        for (k, t) in take.iter().enumerate() {
            self.pay(&who, k, *t);
        }
    }

    /// Burn buffer shares for their share of equity. Reverts if PRUSD would stop being safe.
    #[payable]
    pub fn withdraw_junior(&mut self, shares: U128) {
        one_yocto();
        let who = env::predecessor_account_id();
        let j = self.junior.get(&who).copied().unwrap_or(0);
        require!(j >= shares.0, "shares");
        let outs = ok(self.core.withdraw_junior(shares.0));
        self.junior.insert(who.clone(), j - shares.0);
        for (k, o) in outs.iter().enumerate() {
            self.pay(&who, k, *o);
        }
    }

    /// Take unused credit back out.
    #[payable]
    pub fn withdraw(&mut self, leg: u8, amount: U128) {
        one_yocto();
        let who = env::predecessor_account_id();
        let v = self.credit(&who, leg);
        require!((leg as usize) < self.legs.len() && v >= amount.0, "credit");
        self.credits.insert((who.clone(), leg), v - amount.0);
        self.pay(&who, leg as usize, amount.0);
    }

    #[private]
    pub fn on_paid(&mut self, account: AccountId, leg: u8, amount: U128, #[callback_result] paid: Result<(), PromiseError>) {
        if paid.is_err() {
            self.add_credit(&account, leg, amount.0);
        }
    }

    #[payable]
    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        one_yocto();
        let _ = memo;
        let from = env::predecessor_account_id();
        let b = self.balances.get(&from).copied().unwrap_or(0);
        require!(amount.0 > 0 && b >= amount.0 && from != receiver_id, "transfer");
        self.balances.insert(from, b - amount.0);
        let r = self.balances.get(&receiver_id).copied().unwrap_or(0) + amount.0;
        self.balances.insert(receiver_id, r);
    }

    // ---- views ------------------------------------------------------------------------------

    pub fn state(&self) -> Value {
        let c = &self.core;
        let n = self.legs.len() as u128;
        json!({
            "legs": self.legs,
            "holdings": c.h.iter().map(|x| U128(*x)).collect::<Vec<_>>(),
            "prusd_supply": U128(c.s),
            "buffer_shares": U128(c.j),
            "buffer_equity": U128(c.equity()),
            "safe_against_any_one_leg_failing": c.safe(),
            "buffer_bps": U128(if c.s == 0 { 0 } else { c.equity() * 10_000 / c.s }),
            "buffer_bps_needed_when_balanced": U128(10_000 / (n - 1)),
            "mint_fee_bps": U128(core::FEE_BPS),
        })
    }

    /// Why a call would be rejected, or what it would do, computed by the same code on the live state.
    pub fn check_mint(&self, coins: U128) -> String {
        match self.core.clone().mint(coins.0) {
            Ok(per) => format!("ok: takes {} from each leg", per),
            Err(e) => format!("rejected: {}", e),
        }
    }
    pub fn check_redeem(&self, coins: U128, take: Vec<U128>) -> String {
        let take: Vec<u128> = take.iter().map(|t| t.0).collect();
        match self.core.clone().redeem(coins.0, &take) {
            Ok(()) => format!("ok: pays {}", take.iter().sum::<u128>()),
            Err(e) => format!("rejected: {}", e),
        }
    }
    pub fn check_withdraw_junior(&self, shares: U128) -> String {
        match self.core.clone().withdraw_junior(shares.0) {
            Ok(outs) => format!("ok: pays {:?}", outs),
            Err(e) => format!("rejected: {}", e),
        }
    }

    /// A valid `take` for `redeem`. `avoid_leg` leaves out a leg you do not want to be paid in.
    pub fn quote_redeem(&self, coins: U128, avoid_leg: Option<u8>) -> Vec<U128> {
        self.core.quote(coins.0, avoid_leg.map(|k| k as usize)).into_iter().map(U128).collect()
    }

    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        U128(self.balances.get(&account_id).copied().unwrap_or(0))
    }
    pub fn buffer_shares_of(&self, account_id: AccountId) -> U128 {
        U128(self.junior.get(&account_id).copied().unwrap_or(0))
    }
    pub fn credit_of(&self, account_id: AccountId, leg: u8) -> U128 {
        U128(self.credit(&account_id, leg))
    }
    pub fn ft_total_supply(&self) -> U128 {
        U128(self.core.s)
    }
    pub fn ft_metadata(&self) -> Value {
        json!({ "spec": "ft-1.0.0", "name": "Pro Rata Dollar", "symbol": "PRUSD", "decimals": 6 })
    }
}
