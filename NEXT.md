# Where this stands and what to do next (written 15 November 2026)

## Live now
- PRUSD v1 on Base mainnet: `0x48d55161357bd8782833ff80bf33e5e30846aa30` ($0.40 USDC in reserve).
- PRUSD (three reserves) on NEAR testnet: `coin.prusd-mwgt5ryd.testnet`, keys deleted.
- The Window on NEAR testnet: `pool.window-mwhc6re2.testnet`, keys deleted.
- Boards (work without any setup):
  - https://htmlpreview.github.io/?https://github.com/Ashioya-ui/susds/blob/main/docs/window.html
  - https://htmlpreview.github.io/?https://github.com/Ashioya-ui/susds/blob/main/docs/index.html
- Shadow run log: `docs/shadow.json`. Add an hour: `python3 near/window/data/live.py step`, then commit.

## To run with real money: one command

```
git clone https://github.com/Ashioya-ui/susds        # or download the ZIP from GitHub
cd susds
node script/run_mainnet.mjs --dry                    # shows the plan, simulates every step, sends nothing
node script/run_mainnet.mjs                          # does it
```

Needs Node 18 or newer (nodejs.org) and one private key. Nothing else: no ETH, no other tools.

The key. The script looks for a file holding the key (0x and 64 hex digits) at `./sender.key`,
`~/sender.key`, `~/keys/sender.key` or `~/prusd.key`, or takes `--key <file>` or the `PRUSD_KEY`
environment variable. Use the holder key for `0xd1da74fda379cb622305c9ed04a96e3432c9998f`: that address
holds $0.60 USDC and the $0.30 of v1 coins, which is enough. If no key is found, the script makes a new
one at `~/prusd.key` and prints the address to send USDC to (at least $0.60 on Base).

What it does, skipping anything already done:
1. Deploys PRUSD v2 at `0x2496d819eab766df12214b26ffb7aec4a0b735a2`.
2. In one operation: redeems the v1 coins, funds the v2 buffer with $0.10 and mints $0.30 of v2 PRUSD.
3. Deploys the mobile money escrow at the address in `script/RAMP_ADDRESS`.

Gas is paid in USDC from the same key through the sponsor at api.liquidagent.ai (about $0.04 per step;
capped at $0.12 per step and $0.30 in total). Each step is simulated on live Base state before it is
sent. Results are saved to `script/mainnet_run.json`. If a step fails, run the script again; finished
steps are skipped.

Checked on 15 November: all three steps simulate on live state for the holder address, the sponsor
quoted $0.0359 for step 1, and the payment signature format is accepted by the real USDC contract.
Not yet exercised: the paid part of the sponsor flow, which follows the sponsor's published example.

The Window has no Base version yet. Porting `near/window/src/core.rs` to Solidity and deploying it over
USDC, USDT and DAI on Base is the step that makes it run on real coins.

## To keep the shadow run going by itself
Add the file `.github/workflows/shadow-run.yml` (kept outside the repo; ask for it) through GitHub's web
editor. It runs `live.py step` every hour on GitHub's servers and commits the log.

## Applications
Tracker with 13 openings and 8 drafted messages: `paid-work-tracker-2026-11-15.xlsx` (sent in chat).
Circle Developer Grants form answers are in the chat; the video, transcript, screenshots and deck are not made.
