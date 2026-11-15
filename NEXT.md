# Where this stands and what to do next (written 15 November 2026)

## Live now
- PRUSD v1 on Base mainnet: `0x48d55161357bd8782833ff80bf33e5e30846aa30` ($0.40 USDC in reserve).
- PRUSD (three reserves) on NEAR testnet: `coin.prusd-mwgt5ryd.testnet`, keys deleted.
- The Window on NEAR testnet: `pool.window-mwhc6re2.testnet`, keys deleted.
- Boards (work without any setup):
  - https://htmlpreview.github.io/?https://github.com/Ashioya-ui/susds/blob/main/docs/window.html
  - https://htmlpreview.github.io/?https://github.com/Ashioya-ui/susds/blob/main/docs/index.html
- Shadow run log: `docs/shadow.json`. Add an hour: `python3 near/window/data/live.py step`, then commit.

## To run with real money (needs the laptop)
The funded keys are only on the laptop: the holder key (`0xd1da74fda379cb622305c9ed04a96e3432c9998f`, at
`/home/claude/keys/sender.key` in the other session) and the wallet that pays the gas sponsor.
First confirm the holder key file still exists. Then, in a session on the laptop:

1. Deploy PRUSD v2: send `script/gas_req_prusd.json` to `POST https://api.liquidagent.ai/v1/gas`.
   Expect code at `0x2496d819eab766df12214b26ffb7aec4a0b735a2`.
2. Migrate in one sponsored operation from the holder: `script/gas_req_migrate.json`
   (v1.redeem 300000, usdc.approve, v2.fund 100000, v2.mint 300000).
3. Deploy the mobile money escrow: `script/gas_req_ramp.json`.
4. Simulate every step first with `python3 script/sim_ramp.py` (uses eth_simulateV1 on live Base state).
About $0.12 in sponsor fees in total.

The Window has no Base version yet. Porting `near/window/src/core.rs` to Solidity and deploying it over
USDC, USDT and DAI on Base is the step that makes it run on real coins.

## To keep the shadow run going by itself
Add the file `.github/workflows/shadow-run.yml` (kept outside the repo; ask for it) through GitHub's web
editor. It runs `live.py step` every hour on GitHub's servers and commits the log.

## Applications
Tracker with 13 openings and 8 drafted messages: `paid-work-tracker-2026-11-15.xlsx` (sent in chat).
Circle Developer Grants form answers are in the chat; the video, transcript, screenshots and deck are not made.
