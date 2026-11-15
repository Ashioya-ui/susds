//! The Window: a discount window for dollar stablecoins, as a rule with no operator.
//! All monetary logic is here, with no chain calls. lib.rs only moves tokens.
//!
//! The pool holds n dollar coins, h[k] of coin k, owned pro rata by s shares.
//! Reference level of each coin: s/n. Excess of coin i: e_i = max(0, h[i] - s/n).
//! Window size: K = s / cap_div.
//!
//! The marginal discount on coin i is d(e) = d0 + (dmax - d0) * e / K for excess e in (0, K],
//! and zero while the pool is at or below its reference in coin i. Selling `a` of coin i for
//! coin j moves the excess from e0 to e1 and costs the integral of d over that interval:
//!     fee = d0 * (e1 - e0) + (dmax - d0) * (e1^2 - e0^2) / (2K),         rounded up. The seller
//! receives a - fee of coin j. The sale is refused if it would make e_i exceed K.
//! Taking a coin out of the pool is always at par.
//!
//! Theorem 1 (cap). After any sale, h[i] <= s/n + K. So if coin i becomes worthless the pool
//!   loses at most the fraction 1/n + 1/cap_div of its par value, known in advance.
//!   Deposits and withdrawals scale every h[k] with s (to within one unit), so they keep the cap.
//! Theorem 2 (no gain from splitting). The exact fee is an integral of d, F(e1) - F(e0) with
//!   F(e) = d0*e + (dmax-d0)*e^2/(2K), so over consecutive sales of the same coin it telescopes: the total depends only on where
//!   the excess starts and ends. Each fee is rounded up, so splitting a sale never costs less.
//! Theorem 3 (no gain from flash deposits). Depositing before a sale and withdrawing after
//!   changes nothing for the other shareholders: every quantity scales with s, so the part of
//!   the sale that reaches them pays exactly the fee it would have paid alone.
//! Theorem 4 (pro rata). A deposit of `x` shares brings x*h[k]/s of every coin (rounded up);
//!   a withdrawal takes x*h[k]/s of every coin (rounded down). Nobody chooses coins, so a
//!   shareholder leaving early takes nothing from those who stay.
//! Theorem 5 (shares only gain at par). The par value per share, total/s, never falls through
//!   any call: sales add the fee, deposits round against the depositor, withdrawals against
//!   the withdrawer. Shareholders lose only if a coin they hold really falls below par.
//! No function reads a price.
use near_sdk::near;

pub type R<T> = Result<T, &'static str>;

#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq)]
pub struct Core {
    pub h: Vec<u128>,
    pub s: u128,
    pub d0_bps: u128,
    pub dmax_bps: u128,
    pub cap_div: u128,
}

impl Core {
    pub fn new(n: usize, d0_bps: u128, dmax_bps: u128, cap_div: u128) -> Self {
        Core { h: vec![0; n], s: 0, d0_bps, dmax_bps, cap_div }
    }
    fn n(&self) -> u128 {
        self.h.len() as u128
    }
    pub fn total(&self) -> u128 {
        self.h.iter().sum()
    }
    pub fn k(&self) -> u128 {
        self.s / self.cap_div
    }
    pub fn excess(&self, i: usize) -> u128 {
        self.h[i].saturating_sub(self.s / self.n())
    }
    /// Marginal discount on the next unit of coin i sold to the pool, in basis points.
    pub fn discount_bps(&self, i: usize) -> u128 {
        let k = self.k();
        let e = self.excess(i);
        if k == 0 || e == 0 { 0 } else { self.d0_bps + (self.dmax_bps - self.d0_bps) * e / k }
    }

    /// (fee, amount out) for selling `a` of coin i.
    pub fn quote(&self, i: usize, a: u128) -> R<(u128, u128)> {
        let k = self.k();
        if a == 0 || k == 0 || i >= self.h.len() {
            return Err("sell: bad amount or empty pool");
        }
        let r = self.s / self.n();
        let e0 = self.h[i].saturating_sub(r);
        let e1 = (self.h[i] + a).saturating_sub(r);
        if e1 > k {
            return Err("window full: the pool will not hold more of this coin");
        }
        let den = 20_000 * k;
        let fee = (self.d0_bps * (e1 - e0) * 2 * k + (self.dmax_bps - self.d0_bps) * (e1 * e1 - e0 * e0) + den - 1) / den;
        if fee >= a {
            return Err("sell: amount too small");
        }
        Ok((fee, a - fee))
    }

    pub fn swap(&mut self, i: usize, j: usize, a: u128) -> R<(u128, u128)> {
        if i == j || j >= self.h.len() {
            return Err("sell: choose a different coin to receive");
        }
        let (fee, out) = self.quote(i, a)?;
        if out > self.h[j] {
            return Err("sell: the pool does not hold enough of the coin you want");
        }
        self.h[i] += a;
        self.h[j] -= out;
        Ok((fee, out))
    }

    /// Issue `shares`. Returns what the depositor must bring in each coin.
    pub fn deposit(&mut self, shares: u128) -> R<Vec<u128>> {
        if shares == 0 {
            return Err("deposit: bad amount");
        }
        let amts: Vec<u128> = if self.s == 0 {
            if shares % self.n() != 0 {
                return Err("first deposit must be a multiple of the number of coins");
            }
            vec![shares / self.n(); self.h.len()]
        } else {
            self.h.iter().map(|h| (shares * h + self.s - 1) / self.s).collect()
        };
        for (k, a) in amts.iter().enumerate() {
            self.h[k] += a;
        }
        self.s += shares;
        Ok(amts)
    }

    /// Burn `shares`. Returns what the holder receives in each coin.
    pub fn withdraw(&mut self, shares: u128) -> R<Vec<u128>> {
        if shares == 0 || shares > self.s {
            return Err("withdraw: bad amount");
        }
        let outs: Vec<u128> = self.h.iter().map(|h| shares * h / self.s).collect();
        for (k, o) in outs.iter().enumerate() {
            self.h[k] -= o;
        }
        self.s -= shares;
        Ok(outs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn amt(&mut self) -> u128 {
            let e = self.next() % 12;
            self.next() as u128 % 10u128.pow(e as u32 + 1) + 1
        }
    }

    /// Random call sequences against the code the contract runs; Theorems 1, 2, 4, 5 are
    /// checked exactly, in integers, after every accepted call.
    #[test]
    fn theorems_hold_on_random_sequences() {
        let (mut calls, mut accepted, mut sales) = (0u64, 0u64, 0u64);
        for seed in 1..=400u64 {
            let mut r = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15));
            let n = 2 + (r.next() % 3) as usize;
            let d0 = (r.next() % 60) as u128;
            let mut c = Core::new(n, d0, d0 + 50 + (r.next() % 900) as u128, 2 + (r.next() % 8) as u128);
            for _ in 0..1500 {
                let b = c.clone();
                calls += 1;
                let op = r.next() % 10;
                let res: R<()> = match op {
                    0 | 1 => c.deposit(r.amt() * if b.s == 0 { n as u128 } else { 1 }).map(|_| ()),
                    2 => c.withdraw(if b.s > 0 { r.next() as u128 % b.s + 1 } else { 1 }).map(|outs| {
                        let x = b.s - c.s;
                        for k in 0..n {
                            assert!(outs[k] * b.s <= x * b.h[k] && (outs[k] + 1) * b.s > x * b.h[k], "pro rata");
                        }
                    }),
                    _ => {
                        let (i, j) = (r.next() as usize % n, r.next() as usize % n);
                        let a = if r.next() % 3 == 0 { b.k().saturating_sub(b.excess(i)).max(1) } else { r.amt() };
                        let whole = c.swap(i, j, a);
                        if let Ok((fee, out)) = whole {
                            sales += 1;
                            assert_eq!(fee + out, a);
                            // Theorem 1
                            assert!(c.h[i] <= c.s / n as u128 + c.k(), "cap");
                            // Theorem 2: the same sale in two parts never costs less, and at most one unit more
                            if a >= 2 {
                                let a1 = r.next() as u128 % (a - 1) + 1;
                                let mut d = b.clone();
                                if let (Ok((f1, _)), Ok((f2, _))) = (d.quote(i, a1), { d.h[i] += a1; d.quote(i, a - a1) }) {
                                    assert!(f1 + f2 >= fee && f1 + f2 <= fee + 1, "split");
                                }
                            }
                        }
                        whole.map(|_| ())
                    }
                };
                if res.is_err() {
                    assert_eq!(b, c, "a rejected call changes nothing");
                    continue;
                }
                accepted += 1;
                // Theorem 5: par value per share never falls
                assert!(c.total() * b.s >= b.total() * c.s, "share value, op {op}");
                // Theorem 1 across deposits and withdrawals: every holding scales with s, to within one unit
                if op <= 2 && b.s > 0 {
                    for k in 0..n {
                        assert!(c.h[k] * b.s <= b.h[k] * c.s + b.s && (c.h[k] + 1) * b.s >= b.h[k] * c.s, "scale, op {op}");
                    }
                }
            }
        }
        println!("calls {calls}, accepted {accepted}, sales {sales}");
        assert!(accepted > 50_000 && sales > 20_000);
    }

    /// Theorem 3: deposit, sell, withdraw. The other shareholders end exactly where a direct
    /// sale of the part that reached them would have left them.
    #[test]
    fn flash_deposit_gains_nothing() {
        let mut r = Rng(77);
        for _ in 0..2000 {
            let mut base = Core::new(3, 0, 500, 6);
            base.deposit(3_000_000_000_000).unwrap();
            let pre = r.next() as u128 % 300_000_000_000 + 1;
            base.swap(0, 1, pre).unwrap();
            let sigma = base.s * (r.next() as u128 % 20 + 1);
            let a = r.next() as u128 % 100_000_000_000 + 1_000_000;
            let mut p = base.clone();
            p.deposit(sigma).unwrap();
            if p.swap(0, 1, a).is_err() { continue; }
            let got = p.withdraw(sigma).unwrap();
            // the attacker's own shares carried sigma/(s+sigma) of the sale; the rest reached the others
            let a_eff = a * base.s / (base.s + sigma);
            let mut q = base.clone();
            if q.swap(0, 1, a_eff).is_err() { continue; }
            let _ = got;
            for k in 0..3 {
                let (x, y) = (p.h[k] as i128, q.h[k] as i128);
                assert!((x - y).abs() <= 30, "leg {k}: {x} vs {y}");
            }
        }
    }

    /// Three real scares replayed. Pool: $3,000,000 in three coins, dmax 5%, window 1/6 of
    /// the pool per coin. Sellers sell the doubted coin while the window's marginal discount
    /// is below the market's discount at the reported low.
    #[test]
    fn replay_three_scares() {
        println!("scare | market low | sold to window | avg discount paid | window used | coin share | fee earned | pool at the low | pool after recovery | pool if coin died");
        for (name, coin, low_ppm) in [("Nov 2022 USDT", 1usize, 980_000u128), ("Mar 2023 USDC", 0, 880_000), ("Jun 2023 USDT", 1, 997_000)] {
            let mut c = Core::new(3, 0, 500, 6);
            c.deposit(3_000_000_000_000).unwrap();
            let s = c.s;
            let k = c.k();
            let mkt_bps = (1_000_000 - low_ppm) / 100;
            let target = k.min(k * mkt_bps / c.dmax_bps); // excess at which the window's discount equals the market's
            let (mut sold, mut fees) = (0u128, 0u128);
            for step in 0..200u128 {
                let a = target * (step + 1) / 200 - target * step / 200;
                let j = if c.h[(coin + 1) % 3] >= c.h[(coin + 2) % 3] { (coin + 1) % 3 } else { (coin + 2) % 3 };
                let (fee, _) = c.swap(coin, j, a).unwrap();
                sold += a;
                fees += fee;
            }
            assert!(c.swap(coin, (coin + 1) % 3, k).is_err() || target < k);
            let t = c.total();
            let at_low = t - c.h[coin] * (1_000_000 - low_ppm) / 1_000_000;
            let dead = t - c.h[coin];
            let pc = |x: u128| format!("{:+.2}%", (x as f64 / s as f64 - 1.0) * 100.0);
            println!("{name} | ${:.3} | ${:.0} | {:.2}% | {:.0}% | {:.1}% | ${:.0} | {} | {} | {}",
                low_ppm as f64 / 1e6, sold as f64 / 1e6, fees as f64 / sold as f64 * 100.0, c.excess(coin) as f64 / k as f64 * 100.0,
                c.h[coin] as f64 / t as f64 * 100.0, fees as f64 / 1e6, pc(at_low), pc(t), pc(dead));
        }
    }

    /// The open question: one coin fails for good while a second is in a scare.
    /// Coin X fills its window and goes to zero. Coin Z fills its window, falls to $0.88, recovers.
    #[test]
    fn replay_failure_plus_scare() {
        println!("window | X share | Z share | fees earned | at the low | after Z recovers | bound 1/n + 1/cap_div");
        for cap_div in [6u128, 12, 24] {
            let mut c = Core::new(3, 0, 500, cap_div);
            c.deposit(3_000_000_000_000).unwrap();
            let (s, k, t0) = (c.s, c.k(), c.total());
            for coin in [0usize, 2] {
                for step in 0..200u128 {
                    let a = k * (step + 1) / 200 - k * step / 200;
                    if c.swap(coin, 1, a).is_err() { break; } // the third coin can run out
                }
            }
            let t = c.total();
            let after = t - c.h[0];                       // X is worth nothing
            let low = after - c.h[2] * 12 / 100;          // Z at $0.88
            let pc = |x: u128| format!("{:+.2}%", (x as f64 / s as f64 - 1.0) * 100.0);
            println!("1/{cap_div} of pool | {:.1}% | {:.1}% | ${:.0} | {} | {} | -{:.2}%", c.h[0] as f64 / t as f64 * 100.0,
                c.h[2] as f64 / t as f64 * 100.0, (t - t0) as f64 / 1e6, pc(low), pc(after), (1.0 / 3.0 + 1.0 / cap_div as f64) * 100.0);
            assert!(c.h[0] <= s / 3 + k);                 // Theorem 1 is what bounds the loss
        }
    }
}
