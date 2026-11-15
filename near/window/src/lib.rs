//! The Window on NEAR: a discount window for dollar stablecoins with no operator, no price
//! feed, no pause and no upgrade key. The rules and proofs are in core.rs.
//!
//! Sell a coin to the pool in one call:
//!     coin.ft_transfer_call(receiver = this contract, amount, msg = "sell:<leg to receive>:<min out>")
//! Any other `ft_transfer_call` becomes the sender's credit, used by `deposit`.
//! Payouts go by `ft_transfer`; one that fails is returned to the receiver's credit.
pub mod core;
use crate::core::Core;
use near_sdk::json_types::U128;
use near_sdk::serde_json::{json, Value};
use near_sdk::store::LookupMap;
use near_sdk::{env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError, PromiseOrValue};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Window {
    legs: Vec<AccountId>,
    core: Core,
    shares: LookupMap<AccountId, u128>,
    credits: LookupMap<(AccountId, u8), u128>,
    fees: u128,
}

fn ok<T>(r: Result<T, &'static str>) -> T {
    r.unwrap_or_else(|e| env::panic_str(e))
}
fn one_yocto() {
    require!(env::attached_deposit() == NearToken::from_yoctonear(1), "attach 1 yoctoNEAR");
}

#[near]
impl Window {
    #[init]
    pub fn new(legs: Vec<AccountId>, d0_bps: U128, dmax_bps: U128, cap_div: U128) -> Self {
        require!((2..=4).contains(&legs.len()), "2 to 4 coins");
        for (i, a) in legs.iter().enumerate() {
            require!(!legs[..i].contains(a), "coins must differ");
        }
        require!(d0_bps.0 <= dmax_bps.0 && dmax_bps.0 <= 2_000 && cap_div.0 >= 2, "parameters");
        Self { core: Core::new(legs.len(), d0_bps.0, dmax_bps.0, cap_div.0), legs, shares: LookupMap::new(b"s"), credits: LookupMap::new(b"c"), fees: 0 }
    }

    fn credit(&self, a: &AccountId, k: u8) -> u128 {
        self.credits.get(&(a.clone(), k)).copied().unwrap_or(0)
    }
    fn add_credit(&mut self, a: &AccountId, k: u8, x: u128) {
        let v = self.credit(a, k) + x;
        self.credits.insert((a.clone(), k), v);
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

    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128> {
        let who = env::predecessor_account_id();
        let i = self.legs.iter().position(|a| *a == who).unwrap_or_else(|| env::panic_str("not one of the pool's coins"));
        let p: Vec<&str> = msg.split(':').collect();
        if p.len() == 3 && p[0] == "sell" {
            let j: usize = p[1].parse().unwrap_or_else(|_| env::panic_str("sell:<leg>:<min out>"));
            let min_out: u128 = p[2].parse().unwrap_or_else(|_| env::panic_str("sell:<leg>:<min out>"));
            let (fee, out) = ok(self.core.swap(i, j, amount.0));
            require!(out >= min_out, "less than your minimum");
            self.fees += fee;
            env::log_str(&format!("sell {} leg {} amount {} fee {} out {} leg {}", sender_id, i, amount.0, fee, out, j));
            self.pay(&sender_id, j, out);
        } else {
            self.add_credit(&sender_id, i as u8, amount.0);
        }
        PromiseOrValue::Value(U128(0))
    }

    /// Issue `shares` against credit: your pro-rata part of every coin the pool holds.
    pub fn deposit(&mut self, shares: U128) -> Vec<U128> {
        let who = env::predecessor_account_id();
        let amts = ok(self.core.deposit(shares.0));
        for (k, a) in amts.iter().enumerate() {
            let v = self.credit(&who, k as u8);
            require!(v >= *a, "not enough credit: send each coin with ft_transfer_call first");
            self.credits.insert((who.clone(), k as u8), v - a);
        }
        let s = self.shares.get(&who).copied().unwrap_or(0) + shares.0;
        self.shares.insert(who, s);
        amts.into_iter().map(U128).collect()
    }

    /// Burn `shares` for the same fraction of every coin.
    #[payable]
    pub fn withdraw_shares(&mut self, shares: U128) {
        one_yocto();
        let who = env::predecessor_account_id();
        let s = self.shares.get(&who).copied().unwrap_or(0);
        require!(s >= shares.0, "shares");
        let outs = ok(self.core.withdraw(shares.0));
        self.shares.insert(who.clone(), s - shares.0);
        for (k, o) in outs.iter().enumerate() {
            self.pay(&who, k, *o);
        }
    }

    #[payable]
    pub fn withdraw_credit(&mut self, leg: u8, amount: U128) {
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

    // ---- views ------------------------------------------------------------------------------

    pub fn state(&self) -> Value {
        let c = &self.core;
        let (k, t) = (c.k(), c.total());
        json!({
            "coins": self.legs,
            "holdings": c.h.iter().map(|x| U128(*x)).collect::<Vec<_>>(),
            "shares": U128(c.s),
            "par_value_per_share_ppm": U128(if c.s == 0 { 1_000_000 } else { t * 1_000_000 / c.s }),
            "window_size": U128(k),
            "window_used": (0..c.h.len()).map(|i| U128(c.excess(i))).collect::<Vec<_>>(),
            "discount_now_bps": (0..c.h.len()).map(|i| U128(c.discount_bps(i))).collect::<Vec<_>>(),
            "d0_bps": U128(c.d0_bps), "dmax_bps": U128(c.dmax_bps), "cap_div": U128(c.cap_div),
            "fees_earned": U128(self.fees),
        })
    }

    /// What selling `amount` of coin `leg` would pay, or why it would be refused.
    pub fn quote_sell(&self, leg: u8, amount: U128) -> Value {
        match self.core.quote(leg as usize, amount.0) {
            Ok((fee, out)) => json!({ "ok": true, "fee": U128(fee), "out": U128(out), "average_discount_bps": U128(fee * 10_000 / amount.0) }),
            Err(e) => json!({ "ok": false, "reason": e }),
        }
    }

    pub fn shares_of(&self, account_id: AccountId) -> U128 {
        U128(self.shares.get(&account_id).copied().unwrap_or(0))
    }
    pub fn credit_of(&self, account_id: AccountId, leg: u8) -> U128 {
        U128(self.credit(&account_id, leg))
    }
}
