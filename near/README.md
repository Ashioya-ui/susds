# Pro Rata Dollar (PRUSD) on NEAR

PRUSD is a USD stablecoin whose reserve is split equally across three USD stablecoins. The contract keeps any two of the three worth at least the whole PRUSD supply, so if one reserve coin goes to zero, every PRUSD still redeems for one dollar from the other two. Redemption uses no price feed: the holder chooses which reserve coins to receive, up to a pro-rata cap on each. The contract account has no access keys, so nobody, including its author, can change the code or move the reserve.

**Live demo:** https://ashioya-ui.github.io/susds/ (pick USDC, USDT or DAI to collapse and see what the live contract pays)

NEAR Hackathon 2026, track **Real-World Money**. Built 13 to 14 November 2026.

## Added 15 November, after the hackathon deadline: The Window

A second contract in this repo, [`window/`](window/README.md), built on the day after development closed and listed here for completeness. It is a discount window for stablecoins: a pool that buys a doubted coin at a penalty rising with how much it already holds, up to a hard cap, with no price feed. Five properties are proved and checked on 600,000 random calls. It is replayed hour by hour on real Curve 3pool data from Ethereum mainnet for the three scares below (March 2023: pool 11% under par at the worst hour, 0.34% up at the end). Live on NEAR testnet at `pool.window-mwhc6re2.testnet`, keys deleted. [Live board](https://htmlpreview.github.io/?https://github.com/Ashioya-ui/susds/blob/main/docs/window.html) with the real market read from Ethereum.

## The problem

In March 2023 USDC fell to $0.88 and DAI fell with it, because DAI's reserve could be swapped one for one into USDC. A stablecoin that holds one reserve coin at par inherits that coin's depeg. Price oracles do not solve this: the oracle reports the depeg after holders have already raced to redeem, and whoever redeems first is paid in full while the last holders take the loss.

The note *Depegs are holes* (Ashioya, November 2026) shows why. A stablecoin's off-chain redemption is the one link in its price graph that no on-chain trade can bound. So PRUSD does not try to price that link. It is built to survive losing it.

## What the data shows

Curve's 3pool lets holders choose which of USDC, USDT and DAI to take out, as PRUSD does. Three scares, as reported at the time:

| Scare | Holders dumped | Pool share reported | Low | Did the coin fail? |
|---|---|---|---|---|
| Nov 2022 (FTX) | USDT | USDT 85% of the pool | just under $0.98 | No |
| Mar 2023 (SVB) | USDC, and DAI because DAI held USDC | USDT under 7%, USDC and DAI over 46% each | USDC $0.88 | No |
| Jun 2023 | USDT | USDT over 70% | $0.997 | No |

Holders leave a doubted coin fast and in size, a different coin each time, and all three times the coin came back. Two consequences for the design:

- "Holders are leaving coin X" must not trigger anything in the contract. It would have fired wrongly three times out of three, and one large trader can fake it. PRUSD has no trigger: leaving a coin is just a redemption.
- The buffer is an underwriting business. Buffer shareholders sell every holder the right to leave any one reserve coin at one dollar, and are left holding the coin that was dumped. In these three scares that coin recovered, so they would have kept the fee and lost nothing. They lose only if a reserve coin truly fails.

Sources: [CoinDesk, Nov 2022](https://www.coindesk.com/markets/2022/11/10/tether-dominates-curves-3pool-as-dai-usdc-account-for-just-20-of-liquidity/amp/), [CoinDesk, Mar 2023](https://hotfix.coindesk.com/business/2023/03/10/defi-protocol-curves-500m-stablecoin-pool-hammered-as-traders-flee-usdc), [The Block, Jun 2023](https://www.theblock.co/post/234822/tether-depeg-curve-3pool).

## How it works

The reserve is held in `n` legs, one per reserve stablecoin, in equal parts. There are two claims on it:

- **PRUSD**: redeemable at par.
- **Buffer shares**: first loss. They earn the 0.10% mint fee.

The contract enforces one condition, **SAFE**: for every leg, the *other* legs together hold at least the PRUSD supply.

| Rule | What the contract does |
|---|---|
| Mint | Takes `x/n` plus 0.10% from every leg. Reverts unless SAFE holds afterwards. |
| Redeem | Burns `x` PRUSD. The holder chooses the legs: at most `x` in total, at most `x · h[k] / supply` from leg `k`. |
| Fund | Adds reserve in equal parts and issues buffer shares. |
| Buffer withdrawal | Paid in proportion to holdings. Reverts unless SAFE holds afterwards. |

A holder who distrusts one reserve coin redeems from the others. Nobody has to detect or declare a depeg.

## What is proved

The proofs are in the header of [`prusd/src/core.rs`](prusd/src/core.rs), which holds all the monetary logic in about 150 lines with no chain calls.

1. **Single-leg failure.** If SAFE holds and one leg becomes worthless, every PRUSD is still redeemable at par from the remaining legs.
2. **No oracle.** No function reads a price.
3. **Pro rata per leg.** A redemption never lowers `h[k] / supply` for any leg. Redeeming early takes nothing from later redeemers.
4. **Mint gate.** No sequence of calls moves the contract from SAFE to not SAFE. A large mint cannot dilute the buffer, and a failing coin cannot be swapped into the reserve beyond its equal share.
5. **Buffer.** Equity per buffer share never falls through any call.

`cargo test` runs 600,000 random calls against the same code the contract executes and checks statements 1, 3, 4 and 5 in exact integer arithmetic after every accepted call:

```
calls 600000, accepted 382805, leg failures checked 59622
test core::fuzz::theorems_hold_on_random_sequences ... ok
```

## Live on NEAR testnet

| | |
|---|---|
| PRUSD | [`coin.prusd-mwgt5ryd.testnet`](https://testnet.nearblocks.io/address/coin.prusd-mwgt5ryd.testnet) |
| Reserve tokens | `usd-a`, `usd-b`, `usd-c` under `prusd-mwgt5ryd.testnet`: test tokens standing in for USDC, USDT and DAI, no value, open faucet |
| Access keys on the PRUSD account | none: [deleted](https://testnet.nearblocks.io/txns/FR2Q4u8ce3MNT1uo7iiPXy9pMB6TKfiSxYmLGQmmMFM9), so the code cannot be changed by anyone |

What was run, in order:

| Step | Result |
|---|---|
| Mint 0.90 with no buffer | rejected by the mint gate |
| Fund the buffer with 0.60 | [`CX1KArVC…`](https://testnet.nearblocks.io/txns/CX1KArVCkdDfJKRht9CPy8jA64AzTWqrdkhhWgwYBDko) |
| Mint 0.90 | [`Dw3uAnno…`](https://testnet.nearblocks.io/txns/Dw3uAnnomivLXQLyRRDYMfaG4DbCNETWBJ1gGtv7Vvxx) |
| Mint 1.50 more (buffer dilution) | rejected by the mint gate |
| Treat leg C as failed: redeem 0.30, paid 0.15 from A and 0.15 from B | [`BmdMrzNx…`](https://testnet.nearblocks.io/txns/BmdMrzNx8K3y1u2485wtXERKwcXC7A1HtU9BYA1vX1Rk) |
| Redeem 0.30 entirely from leg A (a run on one leg) | rejected: more than the pro-rata share |
| Withdraw the whole buffer | rejected: PRUSD would be exposed |
| Withdraw 0.09 of buffer shares | [`5z4AZ6XZ…`](https://testnet.nearblocks.io/txns/5z4AZ6XZLkFxfDffGcFyC4gzDTbjnpGQ5V7Trecn2TVF) |

## Check it yourself, no account needed

```
./view.sh state
./view.sh check_mint '{"coins":"1500000"}'
./view.sh check_redeem '{"coins":"300000","take":["300000","0","0"]}'
./view.sh quote_redeem '{"coins":"300000","avoid_leg":2}'
./view.sh check_withdraw_junior '{"shares":"510000"}'
```

Expected, on the state left by the steps above:

```
"safe_against_any_one_leg_failing": true, "prusd_supply": "600000", "holdings": ["324008","324008","462750"]
"rejected: mint gate: the buffer is too small for this mint; fund the buffer first"
"rejected: redeem: more than the pro-rata share of a leg"
["150000","150000","0"]
"rejected: buffer locked: this withdrawal would leave PRUSD exposed to the failure of one leg"
```

The `check_*` views run the contract's own logic on the live state and return the reason a call would be rejected. Amounts have 6 decimals.

## Use it (near-cli)

```
P=coin.prusd-mwgt5ryd.testnet
for L in usd-a usd-b usd-c; do T=$L.prusd-mwgt5ryd.testnet
  near call $T faucet '{"amount":"1000000"}' --accountId you.testnet
  near call $T ft_transfer_call '{"receiver_id":"'$P'","amount":"400000","msg":""}' --accountId you.testnet --depositYocto 1 --gas 100000000000000
done
near call $P mint '{"coins":"30000"}' --accountId you.testnet
near view $P quote_redeem '{"coins":"30000","avoid_leg":2}'
near call $P redeem '{"coins":"30000","take":["15000","15000","0"]}' --accountId you.testnet --depositYocto 1 --gas 150000000000000
```

## Build, test, deploy

```
rustup toolchain install 1.86.0 -t wasm32-unknown-unknown
export CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback
cargo +1.86.0 test -p prusd --release -- --nocapture
RUSTFLAGS="-C link-arg=-s" cargo +1.86.0 build --target wasm32-unknown-unknown --release
npm i near-api-js@4.0.4 && node deploy_near.js     # new testnet account, three test dollars, PRUSD, and the steps above
```

## Use: small cross-border payments

PRUSD is meant as the settlement asset for mobile money to mobile money payments (Kenya to Zambia, USA to the Philippines) in amounts too small for card rails. A payout partner who is paid in PRUSD holds a claim on several issuers, not one.

## Limits

- Testnet. The reserve legs here are valueless test tokens; on mainnet they would be the USD stablecoins issued on NEAR.
- The buffer is large: with `n` equal legs it must be `1/(n−1)` of supply (50% for three legs, 33% for four). Buffer holders are paid only the 0.10% mint fee. That is the cost of surviving a total failure without an oracle.
- The reserve coins must not hold each other. In March 2023 DAI fell with USDC because DAI held USDC; a reserve of USDC and DAI is one risk, not two.
- Next step: separate buffer shares per reserve coin, so the fee each one needs becomes an on-chain price for that issuer's risk. Not built.
- It covers the failure of one leg. Two legs failing together are shared pro rata.
- A rejected transaction shows on the explorer as a generic WebAssembly trap; use the `check_*` views for the reason.
- The contract pays for balance storage, and buffer shares are not transferable.
- The proofs are written arguments checked by the randomized test, not machine-checked.

The same coin in single-reserve form is live on Base mainnet ([repo root](../README.md)); 1,560 adversarial call sequences on live state found the buffer-dilution weakness that the mint gate here closes ([`evolve/`](../evolve)).

Author: William Alubokho Ashioya, Nairobi.
