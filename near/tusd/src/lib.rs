//! Test dollar for NEAR testnet only: a NEP-141 token with an open faucet, standing in for
//! USDC as the PRUSD reserve asset. It has no value. On mainnet the reserve token is USDC.
use near_sdk::json_types::U128;
use near_sdk::serde_json::{json, Value};
use near_sdk::store::LookupMap;
use near_sdk::{env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Tusd {
    symbol: String,
    supply: u128,
    balances: LookupMap<AccountId, u128>,
}

#[near]
impl Tusd {
    #[init]
    pub fn new(symbol: String) -> Self {
        Self { symbol, supply: 0, balances: LookupMap::new(b"b") }
    }

    fn bal(&self, a: &AccountId) -> u128 {
        self.balances.get(a).copied().unwrap_or(0)
    }

    fn mv(&mut self, from: &AccountId, to: &AccountId, x: u128) {
        let b = self.bal(from);
        require!(x > 0 && b >= x && from != to, "transfer");
        self.balances.insert(from.clone(), b - x);
        let r = self.bal(to) + x;
        self.balances.insert(to.clone(), r);
    }

    /// Anyone may take up to 1,000 test dollars per call.
    pub fn faucet(&mut self, amount: U128) {
        require!(amount.0 <= 1_000_000_000, "max 1000");
        let who = env::predecessor_account_id();
        let b = self.bal(&who) + amount.0;
        self.balances.insert(who, b);
        self.supply += amount.0;
    }

    #[payable]
    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        require!(env::attached_deposit() == NearToken::from_yoctonear(1), "attach 1 yoctoNEAR");
        let _ = memo;
        self.mv(&env::predecessor_account_id(), &receiver_id, amount.0);
    }

    #[payable]
    pub fn ft_transfer_call(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>, msg: String) -> Promise {
        require!(env::attached_deposit() == NearToken::from_yoctonear(1), "attach 1 yoctoNEAR");
        let _ = memo;
        let sender = env::predecessor_account_id();
        self.mv(&sender, &receiver_id, amount.0);
        Promise::new(receiver_id.clone())
            .function_call(
                "ft_on_transfer".to_string(),
                json!({ "sender_id": sender, "amount": amount, "msg": msg }).to_string().into_bytes(),
                NearToken::from_yoctonear(0),
                Gas::from_tgas(30),
            )
            .then(Promise::new(env::current_account_id()).function_call(
                "ft_resolve_transfer".to_string(),
                json!({ "sender_id": sender, "receiver_id": receiver_id, "amount": amount }).to_string().into_bytes(),
                NearToken::from_yoctonear(0),
                Gas::from_tgas(10),
            ))
    }

    #[private]
    pub fn ft_resolve_transfer(&mut self, sender_id: AccountId, receiver_id: AccountId, amount: U128, #[callback_result] unused: Result<U128, PromiseError>) -> U128 {
        let back = match unused { Ok(u) => u.0.min(amount.0), Err(_) => amount.0 }.min(self.bal(&receiver_id));
        if back > 0 {
            self.mv(&receiver_id, &sender_id, back);
        }
        U128(amount.0 - back)
    }

    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        U128(self.bal(&account_id))
    }

    pub fn ft_total_supply(&self) -> U128 {
        U128(self.supply)
    }

    pub fn ft_metadata(&self) -> Value {
        json!({ "spec": "ft-1.0.0", "name": "Test Dollar (testnet only, no value)", "symbol": self.symbol, "decimals": 6 })
    }
}
