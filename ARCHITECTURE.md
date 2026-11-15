# Architecture of the Pro Rata Dollar after the real-data runs

15 November 2026. Numbering follows the note *Depegs are holes*. In that note **Proposition 9 is the bounded-churn result** for a reserve that moves; the reserve mix and buffer are **Theorem 8**. The real data speaks to both, and to Proposition 6.

Data: 229 hourly readings of Curve's 3pool on Ethereum mainnet (real USDC, USDT, DAI) across three scares, in `near/window/data/3pool.json`.

## 1. What the data says about each result

### Proposition 9, bounded churn: holds, and it bites

`python3 near/window/data/churn.py` reads the pool's composition as a path of reserve weights and compares the real pool with a reserve that follows the same targets but only takes steps that keep its path self-approaching (rule (1) of the note).

| Scare | Real pool: volume moved | Real path obeys rule (1)? | Under the rule: steps taken | Under the rule: volume moved | Bound of Prop. 9 |
|---|---|---|---|---|---|
| Nov 2022 (FTX), 96 h | 287% of the pool | no, 85 hours break it | 14 | 17.9% | 43.2% |
| Mar 2023 (SVB), 84 h | 356% of the pool | no, 83 hours break it | 1 | 0.5% | 1.4% |
| Jun 2023, 48 h | 134% of the pool | no, 45 hours break it | 6 | 9.0% | 14.0% |

Volume is the sum of |step|₁, as in the note. In every case the volume under the rule is below the bound `√2 · min(3.1297 · diameter, 5.3332 · end-to-end)`. An unconstrained pool was turned over one to three and a half times in a few days; the rule cuts that by 93% to 100%. In March 2023 the rule lets the reserve take one step and then holds it still.

### Theorem 8, reserve mix and buffer: holds once legs are counted as Proposition 6 says

Buffer needed for a reserve held in equal thirds to stay worth one dollar per coin at the worst hour, against the theorem's `b = L / (1 − L)`:

| Scare | Worst-hour prices USDC / USDT / DAI | Buffer the data requires | Theorem 8, counting three separate legs | Theorem 8, counting USDC and DAI as one leg |
|---|---|---|---|---|
| Nov 2022 | 1.0000 / 0.9802 / 0.9996 | 0.68% | 0.66% | 0.66% |
| Mar 2023 | 0.8777 / 1.0000 / 0.8840 | 8.63% | 4.25% | 8.63% |
| Jun 2023 | 0.9999 / 0.9965 / 1.0000 | 0.12% | 0.12% | 0.12% |

With one coin doubted, the theorem's buffer is the buffer the data requires. In March 2023 the three-leg count is too low by half, and the count the note prescribes after Theorem 8 ("legs that share a par link count as one leg") is exact.

### Proposition 6, coins joined at par share one deviation: holds

In March 2023 DAI's lowest hourly rate was $0.8821 against $0.8777 for USDC, in the same hours. DAI was joined to USDC at par.

### Lemma 7 and the pro-rata rule

Checked on 600,000 random calls against the contract's code and on the live contracts (see `near/README.md`). The contract run on the same data paid $0.9998, $0.9411 and $0.9999 per redeemed coin at the worst hour of the three scares.

## 2. What changes in the architecture

**A. A reserve leg is an independence class, not a coin.** USDC and DAI are one leg. The deployed three-coin PRUSD therefore had two legs in March 2023, not three, which is why it paid $0.94 and not $1.00. Choosing legs is now a stated design rule: no two legs may share a bank, a custodian or a par link.

**B. Two safety levels, named and priced separately.**

| Level | Guarantee | Needs a price feed | Buffer for n equal legs |
|---|---|---|---|
| Par by choice (deployed on NEAR) | Any holder redeems one dollar of good coins if any one leg fails completely, by not taking that leg | no | 1 / (n − 1): 100% for two legs, 50% for three |
| Par at reserve value (Theorem 8) | The reserve stays worth one dollar per coin after a loss of at most d on one leg | no | L / (1 − L): 6.5% for two legs at d = 12.2% |

The first is what the contract enforces today. The second is twelve times cheaper for the losses actually observed, but a holder who redeems during the scare receives some of the doubted coin at face value. Turning the second into par at market needs a buyer for the doubted coin.

**C. The Window is that buyer.** `near/window/` buys a doubted reserve coin at a capped, rising penalty. Its place in the architecture is between the two levels in B: holders redeem pro rata, sell the doubted coin to the Window, and the penalties go to those who fund it. Whether PRUSD at the Theorem 8 buffer plus a Window gives par at market is not proved; on the March 2023 data the market discount (12%) was above the Window's maximum penalty (5%), so its window filled.

**D. The reserve does not trade during a scare.** Proposition 9 on real data shows that following the market would have turned the reserve over up to 3.5 times, and that rule (1) nearly freezes it. PRUSD's reserve changes only through holders' own redemptions, which cost it nothing, and through rebalancing under rule (1).

**E. Proposed, not built: a stated loss bound per leg.** Replace the total-failure condition by `total − dᵢ · h[i] ≥ supply` for every leg, with `dᵢ` fixed at deployment. This is the Theorem 8 level as a contract.

## 3. Layers

1. **Reserve legs**: independence classes of dollar coins (A), weights `wᵢ ∝ 1/dᵢ` (Theorem 8).
2. **PRUSD core**: atomic par, pro-rata redemption with a cap per leg, mint gate, buffer shares that earn the mint fee (D1 to D3, Lemma 7).
3. **Window**: buyer of last resort for the reserve coins (C).
4. **Reserve moves**: only along a self-approaching path (Proposition 9).
5. **Distribution**: escrowed mobile money leg (Lemma 10), payment sessions for small amounts (Proposition 12).

## 4. What is live

| Piece | Where | State |
|---|---|---|
| PRUSD, one reserve coin | Base mainnet `0x48d5…aa30` | live, $0.40 USDC |
| PRUSD v2 and escrow | Base mainnet | one command: `node script/run_mainnet.mjs` |
| PRUSD, three coins, par by choice | NEAR testnet `coin.prusd-mwgt5ryd.testnet` | live, keys deleted |
| Window | NEAR testnet `pool.window-mwhc6re2.testnet` | live, keys deleted |
| Rule (1) for reserve moves | library, and `churn.py` on real data | not in a deployed contract |
| Level E | | not built |
