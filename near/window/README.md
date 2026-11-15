# The Window

A discount window for dollar stablecoins, as a contract. A pool funded by patient holders buys a doubted stablecoin at a penalty that rises with how much of that coin it already holds, up to a hard cap. No price feed, no operator, no upgrade key.

**Live demo:** https://ashioya-ui.github.io/susds/window.html

## Why

Three stablecoin scares, as reported at the time on Curve's 3pool:

| Scare | Holders dumped | Pool share reported | Low | Did the coin fail? |
|---|---|---|---|---|
| Nov 2022 | USDT | USDT 85% of the pool | just under $0.98 | No |
| Mar 2023 | USDC and DAI | USDT under 7%, USDC and DAI over 46% each | USDC $0.88 | No |
| Jun 2023 | USDT | USDT over 70% | $0.997 | No |

Each time, holders paid to leave a coin that then returned to par. The parties who absorbed the dumped coin were barely paid: Curve charges a few basis points, and Maker's stability module swapped at exactly par, which is how DAI imported USDC's fall. Central banks handle the same situation with one rule: lend freely, at a penalty rate, against good collateral, up to a limit. This contract is that rule with the limit written in.

Sources: [CoinDesk, Nov 2022](https://www.coindesk.com/markets/2022/11/10/tether-dominates-curves-3pool-as-dai-usdc-account-for-just-20-of-liquidity/amp/), [CoinDesk, Mar 2023](https://hotfix.coindesk.com/business/2023/03/10/defi-protocol-curves-500m-stablecoin-pool-hammered-as-traders-flee-usdc), [The Block, Jun 2023](https://www.theblock.co/post/234822/tether-depeg-curve-3pool). The $0.88 low for USDC is the commonly reported figure and is not in those three articles.

## The rule

The pool holds `n` coins, `h[k]` of coin `k`, owned pro rata by `s` shares.

- Reference level of each coin: `s/n`. Excess of coin `i`: `e = max(0, h[i] − s/n)`.
- Window size: `K = s / cap_div`.
- Marginal discount on coin `i`: `d(e) = d0 + (dmax − d0) · e / K`, and zero while the pool is at or below its reference in that coin.
- Selling `a` of coin `i` moves the excess from `e0` to `e1` and costs the integral of `d`:

```
fee = d0·(e1 − e0) + (dmax − d0)·(e1² − e0²) / (2K)      rounded up
```

- The seller receives `a − fee` of a coin of their choice. A sale that would push `e` past `K` is refused.
- Taking a coin out of the pool is always at par.
- Deposits bring, and withdrawals take, the same fraction of every coin. Nobody chooses coins.

## What is proved

Proofs are in the header of [`src/core.rs`](src/core.rs), which holds all the monetary logic with no chain calls.

1. **Cap.** After any sale `h[i] ≤ s/n + K`. If a coin becomes worthless the pool loses at most `1/n + 1/cap_div` of its par value, known in advance.
2. **No gain from splitting.** The fee is an integral, so it telescopes over consecutive sales; each fee is rounded up, so splitting never costs less.
3. **No gain from flash deposits.** Every quantity scales with `s`. Depositing before a sale and withdrawing after leaves the other shareholders exactly where a direct sale of the part that reached them would have.
4. **Pro rata.** A shareholder who leaves early takes nothing from those who stay.
5. **Shares only gain at par.** Par value per share never falls through any call. Shareholders lose only if a coin they hold really falls.

`cargo test -p window --release -- --nocapture` checks 1, 2, 4 and 5 after every accepted call of 600,000 random calls, and 3 on 2,000 random cases:

```
calls 600000, accepted 340697, sales 160582
test result: ok. 4 passed; 0 failed
```

## Replay on real data

`data/3pool.json` holds 229 hourly readings of Curve's 3pool on Ethereum mainnet (real USDC, USDT, DAI) across the three scares: balances and the pool's own exchange rate for a $100,000 trade, read from archive nodes by `data/fetch_3pool.py`. What the chain shows:

| Scare | Weakest coin, lowest hourly rate | USDT share of the pool |
|---|---|---|
| Nov 2022 (FTX) | USDT $0.9802 | up to 86.2% |
| Mar 2023 (SVB) | USDC $0.8777, DAI $0.8821 | down to 1.4% |
| Jun 2023 | USDT $0.9965 | up to 75.3% |

The test `replay_real_3pool_data` runs the contract's code on a $3,000,000 pool against that data. Each hour, holders sell a coin to the window while its marginal penalty is below the discount the market shows.

| Scare | Sold to window | Window used (USDC / USDT / DAI) | Penalties | Pool at its lowest | At the last hour | Once back at $1 |
|---|---|---|---|---|---|---|
| Nov 2022, 96 h | $222,998 | 0% / 39% / 0% | $1,942 | −0.74% | −0.02% | +0.06% |
| Mar 2023, 84 h | $1,000,000 | 100% / 0% / 100% | $25,000 | −11.08% | +0.34% | +0.83% |
| Jun 2023, 48 h | $38,000 | 0% / 7% / 0% | $61 | −0.12% | −0.04% | +0.00% |

March 2023 is the hard case: USDC and DAI were dumped together, both windows filled, and the pool sat 11% under par before ending the period 0.34% up. The pool size is a model input; the prices and balances are not.

## Replays with assumed lows

Pool of $3,000,000 in three coins, `d0 = 0`, `dmax = 5%`, window one sixth of the pool per coin. Sellers sell the doubted coin while the window's marginal discount is below the market's discount at the reported low. Printed by the test `replay_three_scares`:

| Scare | Market low | Sold to window | Avg penalty | Window used | Pool at the low | After recovery | If the coin had died |
|---|---|---|---|---|---|---|---|
| Nov 2022 USDT | $0.980 | $200,000 | 1.00% | 40% | −0.73% | +0.07% | −39.93% |
| Mar 2023 USDC | $0.880 | $500,000 | 2.50% | 100% | −5.58% | +0.42% | −49.58% |
| Jun 2023 USDT | $0.997 | $30,000 | 0.15% | 6% | −0.10% | +0.00% | −34.33% |

One coin fails for good while a second is in a scare (test `replay_failure_plus_scare`): coin X fills its window and goes to zero; coin Z fills its window, falls to $0.88 and recovers.

| Window per coin | At the low | After Z recovers | Bound from theorem 1 |
|---|---|---|---|
| 1/6 of pool | −55.17% | −49.17% | −50.00% |
| 1/12 of pool | −46.25% | −41.25% | −41.67% |
| 1/24 of pool | −41.79% | −37.29% | −37.50% |

So the answer to "does it stay whole through a real failure plus a scare" is no. The pool has no debts, so it cannot become insolvent, but shareholders lose up to the bound. The penalties earned (under 1% of the pool) do not cover a dead coin. The window size is the dial: a smaller window loses less and absorbs less.

## Live on NEAR testnet

| | |
|---|---|
| Contract | [`pool.window-mwhc6re2.testnet`](https://testnet.nearblocks.io/address/pool.window-mwhc6re2.testnet) |
| Coins | three test tokens standing in for USDC, USDT and DAI (no value, open faucet) |
| Parameters | `d0 = 0`, `dmax = 5%`, `cap_div = 6` |
| Access keys | none: [deleted](https://testnet.nearblocks.io/txns/4JiT5r3K49UmAUx59P93pR1JRqQR5mszHquMZxChteV6) |

The three scares were then run on the live contract at 1:3333 scale ($900 pool):

| Step | Discount on next dollar after it (USDC, USDT, DAI) | Penalties earned so far |
|---|---|---|
| [Jun 2023: sell $9 USDT](https://testnet.nearblocks.io/txns/CLf3KtRMi8yMvNz74gY791TL3J6k8Vo43Wje8MKJR52S) | 0%, 0.30%, 0% | $0.0135 |
| [Nov 2022: sell $51 more USDT](https://testnet.nearblocks.io/txns/9wQPBaNn3R174FZksUAbLCrbLVnhBrfWjuW3hrSnmnx3) | 0%, 2.00%, 0% | $0.60 |
| [Mar 2023: sell $150 USDC](https://testnet.nearblocks.io/txns/7SQHGGVYS7hpdDLGkTG2GvyFfNojYfoCng1Zi7Vqhw1S) | 5.00%, 2.00%, 0% | $4.35 |
| Sell $1 more USDC | refused: window full | |
| [Recovery: $30 DAI swapped for USDT at par](https://testnet.nearblocks.io/txns/2duDC5XNPy3PY4xX4Uemn93H7R6EPuHnge6CA1YtTwoU) | 5.00%, 1.00%, 0% | $4.35 |

The penalties match the formula exactly: $60 of USDT at an average 1% and $150 of USDC at an average 2.5%.

## Use it

```
P=pool.window-mwhc6re2.testnet; A=usd-a.prusd-mwgt5ryd.testnet   # usd-a USDC, usd-b USDT, usd-c DAI stand-ins
near view $P state
near view $P quote_sell '{"leg":1,"amount":"10000000"}'
near call $A faucet '{"amount":"10000000"}' --accountId you.testnet
near call usd-b.prusd-mwgt5ryd.testnet faucet '{"amount":"10000000"}' --accountId you.testnet
near call usd-b.prusd-mwgt5ryd.testnet ft_transfer_call '{"receiver_id":"'$P'","amount":"10000000","msg":"sell:2:0"}' --accountId you.testnet --depositYocto 1 --gas 150000000000000
```

`msg = "sell:<leg to receive>:<minimum out>"`. Amounts have 6 decimals.

## Limits

- Testnet, with valueless test tokens.
- The window buys outright; it does not lend. Whether sellers should hold a right to repurchase at par is open.
- After a scare the pool keeps the coin it bought until someone swaps it out at par. There is no incentive to do so, so the window reopens slowly.
- If two coins are dumped for the third, the third runs out. In March 2023 USDC and DAI fell together because DAI held USDC; the coins in a pool should not hold each other.
- The replays use real hourly pool prices but assume sellers use the window whenever it is cheaper than the market; real order flow was not available. The pool size is assumed.
- Proofs are written arguments checked by randomized tests, not machine-checked.

Author: William Alubokho Ashioya, Nairobi.
