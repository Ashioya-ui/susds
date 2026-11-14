//! The whole monetary logic of PRUSD, with no chain calls in it. The contract in lib.rs only
//! moves tokens according to what these functions return, and the fuzz test at the bottom
//! runs these same functions.
//!
//! Model. The reserve is held in n legs (n different dollar stablecoins), h[i] units of leg i.
//! s is the supply of PRUSD (senior, redeemable at par). j is the supply of buffer shares
//! (junior, first loss, earns the mint fee).
//!
//! SAFE means: for every leg i, the other legs together hold at least s.
//! Theorem 1 (single-leg failure). If SAFE holds and any one leg becomes worthless, every
//!   PRUSD can still be redeemed at par from the remaining legs. Proof: the remaining legs
//!   hold total - h[i] >= s, and `redeem` lets x coins take up to x*h[k]/s from each leg k.
//! Theorem 2 (no oracle). No function reads a price. A holder who distrusts leg i simply
//!   takes the other legs; nobody has to detect or declare a depeg.
//! Theorem 3 (pro rata, per leg). `redeem` never lowers h[k]/s for any leg k: it removes
//!   t <= x*h[k]/s, and (h[k]-t)/(s-x) >= h[k]/s. So redeeming early takes nothing from those
//!   who redeem later, leg by leg, and SAFE survives every redemption.
//! Theorem 4 (mint gate). `mint` and `withdraw_junior` revert unless SAFE holds afterwards;
//!   `fund` only adds reserve. So no sequence of calls takes the contract from SAFE to not
//!   SAFE. In particular a large mint cannot dilute the buffer below the safe level, and a
//!   failing coin cannot be swapped into the reserve beyond its weight, because minting
//!   takes all legs in equal parts.
//! Theorem 5 (junior is first loss and is paid for it). Buffer shares are withdrawn in
//!   proportion to holdings, with no choice of leg, and only while SAFE holds afterwards.
//!   The mint fee stays in the reserve, so equity per buffer share never falls through any call.
use near_sdk::near;

pub const FEE_BPS: u128 = 10; // 0.10% of every mint goes to the buffer
pub type R<T> = Result<T, &'static str>;

#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq)]
pub struct Core {
    pub h: Vec<u128>,
    pub s: u128,
    pub j: u128,
}

impl Core {
    pub fn new(n: usize) -> Self {
        Core { h: vec![0; n], s: 0, j: 0 }
    }
    fn n(&self) -> u128 {
        self.h.len() as u128
    }
    pub fn total(&self) -> u128 {
        self.h.iter().sum()
    }
    pub fn equity(&self) -> u128 {
        self.total().saturating_sub(self.s)
    }
    pub fn safe(&self) -> bool {
        let t = self.total();
        self.h.iter().all(|x| t - x >= self.s)
    }

    /// Mint x PRUSD. Returns the amount taken from the caller in each leg: x/n plus the fee.
    pub fn mint(&mut self, x: u128) -> R<u128> {
        if x == 0 || x % self.n() != 0 {
            return Err("mint: amount must be a positive multiple of the number of legs");
        }
        let per = x / self.n();
        let per = per + (per * FEE_BPS + 9_999) / 10_000;
        let mut c = self.clone();
        c.h.iter_mut().for_each(|v| *v += per);
        c.s += x;
        if !c.safe() {
            return Err("mint gate: the buffer is too small for this mint; fund the buffer first");
        }
        *self = c;
        Ok(per)
    }

    /// Add y to the buffer in equal parts. Returns (amount per leg, buffer shares issued).
    pub fn fund(&mut self, y: u128) -> R<(u128, u128)> {
        if y == 0 || y % self.n() != 0 {
            return Err("fund: amount must be a positive multiple of the number of legs");
        }
        let e = self.equity();
        let shares = if self.j == 0 {
            y
        } else if e == 0 {
            return Err("fund: buffer equity is zero");
        } else {
            y * self.j / e
        };
        if shares == 0 {
            return Err("fund: amount too small");
        }
        let per = y / self.n();
        self.h.iter_mut().for_each(|v| *v += per);
        self.j += shares;
        Ok((per, shares))
    }

    /// Burn x PRUSD and take `take[k]` from leg k: at most x in total, at most x*h[k]/s per leg.
    pub fn redeem(&mut self, x: u128, take: &[u128]) -> R<()> {
        if x == 0 || x > self.s || take.len() != self.h.len() {
            return Err("redeem: bad amount");
        }
        if take.iter().sum::<u128>() > x {
            return Err("redeem: more than par");
        }
        for (k, t) in take.iter().enumerate() {
            if *t > x * self.h[k] / self.s {
                return Err("redeem: more than the pro-rata share of a leg");
            }
        }
        for (k, t) in take.iter().enumerate() {
            self.h[k] -= t;
        }
        self.s -= x;
        Ok(())
    }

    /// Burn buffer shares for their share of equity, paid in proportion to holdings.
    pub fn withdraw_junior(&mut self, shares: u128) -> R<Vec<u128>> {
        let t = self.total();
        if shares == 0 || shares > self.j || t == 0 {
            return Err("withdraw: bad amount");
        }
        let v = shares * self.equity() / self.j;
        let outs: Vec<u128> = self.h.iter().map(|h| v * h / t).collect();
        let mut c = self.clone();
        for (k, o) in outs.iter().enumerate() {
            c.h[k] -= o;
        }
        c.j -= shares;
        if !c.safe() {
            return Err("buffer locked: this withdrawal would leave PRUSD exposed to the failure of one leg");
        }
        *self = c;
        Ok(outs)
    }

    /// A valid `take` for redeeming x that avoids leg `skip` (if given) and pays as close to par as allowed.
    pub fn quote(&self, x: u128, skip: Option<usize>) -> Vec<u128> {
        let n = self.h.len();
        if self.s == 0 || x == 0 || x > self.s {
            return vec![0; n];
        }
        let ok = |k: usize| Some(k) != skip;
        let cap: Vec<u128> = (0..n).map(|k| if ok(k) { x * self.h[k] / self.s } else { 0 }).collect();
        let tl: u128 = (0..n).filter(|k| ok(*k)).map(|k| self.h[k]).sum();
        let mut take: Vec<u128> = (0..n).map(|k| if ok(k) && tl > 0 { cap[k].min(x * self.h[k] / tl) } else { 0 }).collect();
        let mut left = x.saturating_sub(take.iter().sum());
        for k in 0..n {
            let add = left.min(cap[k] - take[k]);
            take[k] += add;
            left -= add;
        }
        take
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn amt(&mut self, n: u128) -> u128 {
            let e = self.next() % 13;
            (self.next() as u128 % 10u128.pow(e as u32 + 1) + 1) * if self.next() % 4 == 0 { 1 } else { n }
        }
    }

    /// Random call sequences against the same code the contract runs. After every call that
    /// succeeds, the theorems in the header are checked exactly, in integers.
    #[test]
    fn theorems_hold_on_random_sequences() {
        let (mut calls, mut accepted, mut failures) = (0u64, 0u64, 0u64);
        for seed in 1..=400u64 {
            let mut r = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15));
            let n = 2 + (r.next() % 3) as usize;
            let mut c = Core::new(n);
            for _ in 0..1500 {
                let b = c.clone();
                calls += 1;
                let op = r.next() % 10;
                let res: R<()> = match op {
                    0..=2 => c.mint(r.amt(n as u128)).map(|_| ()),
                    3 | 4 => c.fund(r.amt(n as u128)).map(|_| ()),
                    5..=7 => {
                        let x = if c.s > 0 { r.next() as u128 % c.s + 1 } else { 1 };
                        let take = match r.next() % 4 {
                            0 => c.quote(x, None),
                            1 => c.quote(x, Some(r.next() as usize % n)),
                            2 => (0..n).map(|_| r.next() as u128 % (x + 2)).collect(), // greedy or invalid
                            _ => (0..n).map(|k| if c.s > 0 { x * c.h[k] / c.s } else { 0 }).collect(), // every cap at once
                        };
                        c.redeem(x, &take)
                    }
                    8 => {
                        let sh = if c.j > 0 { r.next() as u128 % c.j + 1 } else { 1 };
                        c.withdraw_junior(sh).map(|_| ())
                    }
                    _ => {
                        // one leg becomes worthless (an event outside the contract)
                        if b.safe() && b.s > 0 {
                            let k = r.next() as usize % n;
                            failures += 1;
                            // Theorem 1: every coin is still redeemable at par without leg k
                            assert!(b.total() - b.h[k] >= b.s);
                            let x = r.next() as u128 % b.s + 1;
                            let q = b.quote(x, Some(k));
                            assert!(q[k] == 0 && q.iter().sum::<u128>() + n as u128 > x, "par without the failed leg");
                            let mut d = b.clone();
                            d.redeem(x, &q).expect("quote is always a valid redemption");
                        }
                        Err("no state change")
                    }
                };
                if res.is_err() {
                    assert_eq!(b, c, "a rejected call changes nothing");
                    continue;
                }
                accepted += 1;
                // Theorem 4: SAFE is never lost
                assert!(!b.safe() || c.safe(), "safe -> safe, op {op}");
                // reserve minus supply never falls except by a buffer withdrawal
                if op != 8 {
                    assert!(c.total() - c.s >= b.total() - b.s);
                }
                // Theorem 3: per-leg backing never falls under redemption
                if (5..=7).contains(&op) {
                    for k in 0..n {
                        assert!(c.h[k] * b.s >= b.h[k] * c.s);
                    }
                    assert!(b.total() - c.total() <= b.s - c.s, "never more than par");
                }
                // Theorem 5: equity per buffer share never falls
                if b.j > 0 && c.j > 0 {
                    assert!(c.equity() * b.j >= b.equity() * c.j, "junior price, op {op}");
                }
            }
        }
        println!("calls {calls}, accepted {accepted}, leg failures checked {failures}");
        assert!(accepted > 50_000 && failures > 5_000);
    }
}
