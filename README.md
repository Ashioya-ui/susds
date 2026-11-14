# Pro Rata Dollar (PRUSD)

**A dollar coin on Base that redeems pro rata, at every hour, with no one in charge.**

| | |
|---|---|
| v1 contract (name sUSDs, retired: the name is Sky's) | [`0x48d55161357bd8782833ff80bf33e5e30846aa30`](https://basescan.org/address/0x48d55161357bd8782833ff80bf33e5e30846aa30) on Base (chain 8453) |
| v2 contract, PRUSD | `0x2496d819eab766df12214b26ffb7aec4a0b735a2` on Base, CREATE2 salt `keccak("PRUSD:v1")`, deploys from `script/gas_req_prusd.json`; not yet broadcast |
| Reserve asset | USDC `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913` |
| Deployed | 13 November 2026, block 56,913,768, [tx `0x006c…dccf`](https://basescan.org/tx/0x006c1050bb77a134bacc0afb53c490de297281bc1f62d4bd2c8ad5b03ac9dccf), CREATE2 with salt `keccak("PRUSD:2026-11-13")`, gas paid by a sponsor in USDC |
| Owner, admin, pause, upgrade | none |
| Decimals | 6 |

## Three rules, one contract

1. **Atomic par.** `mint(x)`: x USDC in, x PRUSD out. `redeem(x)`: x PRUSD in, `x · min(1, B)` USDC out, where `B = reserve / supply`. Both are one call, at any time.
2. **Pro rata.** When `B < 1`, every redemption pays the same fraction, so the first to redeem gains nothing over the last. There is no reason to run.
3. **Buffer.** `fund(y)` adds USDC without minting. `bufferBps()` reports `B − 1`. With one reserve asset and a worst-case loss of 10%, a buffer of 11.11% keeps every redemption at one dollar.

Inside Base, the price of PRUSD on any route stays within the fees of the pools that connect the route to `mint`/`redeem`; arbitrage enforces it every block. What no contract can bound is the link from USDC to the dollar.

## Use it

```
usdc.approve(0x48d55161357bd8782833ff80bf33e5e30846aa30, amount)
susds.mint(amount)        // 1 USDC = 1 PRUSD, 6 decimals
susds.redeem(amount)      // pays amount · min(1, B) USDC
susds.fund(amount)        // adds to the reserve, mints nothing
susds.backingPpm()        // B in parts per million
susds.bufferBps()         // buffer in basis points
```

## In this repository

- `contracts/PRUSD.sol`: the whole contract, 102 lines.
- `script/`: compiled artifact (solc 0.8.26, viaIR, cancun), the gas-sponsor request that deployed it, an offline signer.

The proofs behind the three rules are in the note *Depegs are holes: when an on-chain coin is money* (Ashioya, November 2026), kept separately.

Deployment: `script/gas_req.json` is the body for `POST https://api.liquidagent.ai/v1/gas`. The address depends only on the salt and the code, not on the sender.
