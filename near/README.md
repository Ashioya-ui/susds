# Pro Rata Dollar on NEAR

A USD stablecoin contract. Redemptions are paid pro rata from the reserve. The contract has no owner, no pause and no upgrade key. Built 13 to 14 November 2026 for the NEAR Hackathon, track **Real-World Money**.

**Live on NEAR testnet**

| | |
|---|---|
| PRUSD contract | [`prusd.proratadollar-mwgspqhd.testnet`](https://testnet.nearblocks.io/address/prusd.proratadollar-mwgspqhd.testnet) |
| Reserve token (test dollar, no value, open faucet) | [`tusd.proratadollar-mwgspqhd.testnet`](https://testnet.nearblocks.io/address/tusd.proratadollar-mwgspqhd.testnet) |
| Fund 0.10 | [`ARZgneok…`](https://testnet.nearblocks.io/txns/ARZgneoksULQqdcF7d5AbrVskSLz5jPbp63NxuxEcQwy) |
| Mint 0.30 | [`HKG7hV7h…`](https://testnet.nearblocks.io/txns/HKG7hV7hYYJEKm11jUydnhuDr7qrUNMF96sKLpcYX8Xg) |
| Redeem 0.05 | [`GDuicTV7…`](https://testnet.nearblocks.io/txns/GDuicTV7kL7uBtbUpqj66nr5stiTKNhbnj2YERBFEXXb) |
| State after those calls | reserve 350000, supply 250000, backing 1.40, buffer 40% |

On mainnet the reserve token is USDC; the test dollar exists only so that anyone can try the contract on testnet without asking for funds.

## The problem

Most stablecoins redeem first come, first served. If the reserve is short, early redeemers are paid in full and the last holders take the whole loss. Holders therefore race to redeem, as with USDC and DAI in March 2023.

## Three rules

1. **Atomic par.** Send `x` of the reserve token with `ft_transfer_call(msg = "mint")` and receive `x` PRUSD. `redeem(x)` burns `x` PRUSD and pays `x · min(1, B)`, where `B = reserve / supply`.
2. **Pro rata.** When `B < 1`, every redemption pays `B` per coin, whatever its order.
3. **Buffer.** `ft_transfer_call(msg = "fund")` adds reserve and mints nothing. No method withdraws it.

The three invariants (solvency is preserved; the buffer never shrinks; the backing ratio never falls under redemption) are proved by cases at the top of [`prusd/src/lib.rs`](prusd/src/lib.rs). The contract is about 150 lines.

## Check it yourself, no account needed

```
curl -s https://test.rpc.fastnear.com -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"query","params":{"request_type":"call_function","finality":"final","account_id":"prusd.proratadollar-mwgspqhd.testnet","method_name":"backing_ppm","args_base64":"e30="}}'
```

The `result` bytes decode to `"1400000"`. The other views are `reserve`, `ft_total_supply`, `buffer_bps`, `ft_metadata`, `ft_balance_of`.

## Use it (near-cli)

```
P=prusd.proratadollar-mwgspqhd.testnet; T=tusd.proratadollar-mwgspqhd.testnet
near call $T faucet '{"amount":"1000000"}' --accountId you.testnet
near call $T ft_transfer_call '{"receiver_id":"'$P'","amount":"300000","msg":"mint"}' --accountId you.testnet --depositYocto 1 --gas 100000000000000
near call $P redeem '{"coins":"100000"}' --accountId you.testnet --depositYocto 1 --gas 100000000000000
near view $P backing_ppm
```

Amounts have 6 decimals: `1000000` is one dollar.

## Build and deploy

```
rustup toolchain install 1.86.0 -t wasm32-unknown-unknown
CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback RUSTFLAGS="-C link-arg=-s" \
  cargo +1.86.0 build --target wasm32-unknown-unknown --release
npm i near-api-js@4.0.4 && node deploy_near.js   # creates a testnet account, deploys both contracts, runs fund, mint, redeem
```

## Where this goes: small cross-border payments

The first use is mobile money to mobile money across a border (Kenya to Zambia; USA to the Philippines), settled in PRUSD between two people, for amounts too small for card rails. The same coin runs on Base ([repo root](../README.md)), where 1,560 adversarial call sequences executed against live mainnet state took nothing from holders ([`evolve/`](../evolve)).

## Limits

- Testnet only. The reserve here is a valueless test token.
- No storage deposits: the contract pays for balance storage. Fine for a pilot, to be changed before mainnet.
- If a redemption payout fails, the burn is undone at the original amounts.
- The proofs are written arguments in the source, not machine-checked.

Author: William Alubokho Ashioya, Nairobi.
