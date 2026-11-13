# sUSDs

A stablecoin with an atomic par link, pro-rata redemption and a first-loss buffer, and the two papers it rests on.

- `contracts/SUSDs.sol`: one contract, no owner, no pause, no upgrade. `mint` and `redeem` are one call each against USDC held in the contract; a redemption of x coins pays x · min(1, B) USDC, B = reserve/supply; `fund` adds the buffer. Design D1–D3 of the note below.
- `script/`: compiled artifact (solc 0.8.26, viaIR, cancun) and an offline signer for the Base mainnet deployment. Deployer 0x27e0b6d815641c3a0a0940a8409c37b1fa8043ee; direct deployment at nonce 0 gives 0x6dbb390f95e4d64a80c943fb25fa1d93161e3f27, CREATE2 through 0x4e59b44847b379578588920cA78FbF26c0B4956C with salt keccak("sUSDs:2026-11-13") gives 0x48d55161357bd8782833ff80bf33e5e30846aa30. Simulated on Base mainnet: 830,662 gas.
- `note/`: *Depegs are holes: when an on-chain coin is money* (W. A. Ashioya, 12 November 2026), with its data workbook.
- `paper/`: *The length-over-diameter ratio of self-approaching curves* (E. Langetepe and W. A. Ashioya, November 2026), PDF and arXiv source. Proposition 9 of the note uses its Theorem 4.2.

Status, 13 November 2026: deployment signed, not yet broadcast (the deployer holds no ETH on Base).
