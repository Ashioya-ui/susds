#!/usr/bin/env node
// Real-money run of PRUSD on Base mainnet, from one key, with no ETH.
//
//   node script/run_mainnet.mjs            do it
//   node script/run_mainnet.mjs --dry      show the plan, simulate every step on live state, get the fee quotes; send nothing
//   node script/run_mainnet.mjs --dry --address 0x...   the same without a key
//
// Key: env PRUSD_KEY, or --key <file>, or one of ./sender.key, ~/sender.key, ~/keys/sender.key, ~/prusd.key
// (a file holding 0x and 64 hex digits). If there is none, a new key is made at ~/prusd.key and the
// script tells you what to send to it. The key is never printed.
//
// What it does, skipping whatever is already done:
//   1. deploys PRUSD v2 (CREATE2, address fixed in script/PRUSD_ADDRESS)
//   2. redeems the old v1 coins, funds the v2 buffer and mints v2 PRUSD, in one operation
//   3. deploys the mobile money escrow (script/RAMP_ADDRESS)
// Every step is simulated with eth_simulateV1 on live Base state before it is sent. Gas is paid by the
// sponsor at api.liquidagent.ai in USDC taken from the same key (x402, EIP-3009), so no ETH is needed.
// Fee caps: $0.12 per step, $0.30 in total. Needs Node 18 or newer and nothing else.
import { readFileSync, writeFileSync, existsSync, chmodSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
import { homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { keccak256, signDigest, addressOfKey, newKey, calldata, recoverAddress, signTypedDataWithKey } from './evm.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const arg = (k) => { const i = process.argv.indexOf('--' + k); return i > -1 ? process.argv[i + 1] : null; };
const DRY = process.argv.includes('--dry');
const RPCS = ['https://mainnet.base.org', 'https://base.drpc.org', 'https://base-rpc.publicnode.com'];
const API = 'https://api.liquidagent.ai/v1/gas';
const USDC = '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913', V1 = '0x48d55161357bd8782833ff80bf33e5e30846aa30';
const V2 = readFileSync(join(HERE, 'PRUSD_ADDRESS'), 'utf8').trim().toLowerCase(), RAMP = readFileSync(join(HERE, 'RAMP_ADDRESS'), 'utf8').trim().toLowerCase();
const FEE_CAP = 120_000n, FEE_TOTAL_CAP = 300_000n, FEE_RESERVE = 100_000n;      // USDC, 6 decimals
const usd = (x) => '$' + (Number(x) / 1e6).toFixed(4), sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const hx = (b) => '0x' + Buffer.from(b).toString('hex'), buf = (h) => Buffer.from(String(h).replace(/^0x/, ''), 'hex');
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);
const die = (m) => { console.error('\nSTOPPED: ' + m); process.exit(1); };

// ---- key ------------------------------------------------------------------------------------
function loadKey() {
  if (process.env.PRUSD_KEY) return process.env.PRUSD_KEY.trim();
  const files = [arg('key'), 'sender.key', join(homedir(), 'sender.key'), join(homedir(), 'keys', 'sender.key'), join(homedir(), 'prusd.key')].filter(Boolean);
  for (const f of files) if (existsSync(f)) { const m = readFileSync(f, 'utf8').match(/(0x)?[0-9a-fA-F]{64}/); if (m) return (m[1] ? '' : '0x') + m[0]; }
  if (arg('address')) return null;
  const k = newKey(), f = join(homedir(), 'prusd.key');
  writeFileSync(f, k + '\n'); try { chmodSync(f, 0o600); } catch {}
  console.log(`No key found. A new one was saved at ${f} (keep that file private and backed up).\nAddress: ${addressOfKey(k)}\nSend at least $0.60 of USDC on Base to that address, then run this script again.`);
  process.exit(0);
}
const KEY = loadKey();
const ME = (KEY ? addressOfKey(KEY) : arg('address')).toLowerCase();
if (!KEY && !DRY) die('--address works only with --dry');

// ---- chain ----------------------------------------------------------------------------------
let ri = 0;
async function rpc(method, params) {
  let err;
  for (let k = 0; k < 9; k++) {
    try {
      const r = await (await fetch(RPCS[ri % RPCS.length], { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }) })).json();
      if (r.result !== undefined && r.result !== null) return r.result;
      err = JSON.stringify(r.error || r).slice(0, 200);
      if (r.error && /revert|execution/i.test(err)) throw new Error(err);
    } catch (e) { err = e.message; if (/revert|execution/i.test(err)) throw e; }
    ri++; await sleep(400 * (k + 1));
  }
  throw new Error('rpc ' + method + ': ' + err);
}
const call = async (to, sig, args = []) => BigInt(await rpc('eth_call', [{ to, data: calldata(sig, args) }, 'latest']));
const hasCode = async (a) => (await rpc('eth_getCode', [a, 'latest'])).length > 2;
const balances = async () => ({ usdc: await call(USDC, 'balanceOf(address)', [ME]), v1: await call(V1, 'balanceOf(address)', [ME]),
  v2: (await hasCode(V2)) ? await call(V2, 'balanceOf(address)', [ME]) : 0n });
async function simulate(calls, label) {
  const res = await rpc('eth_simulateV1', [{ blockStateCalls: [{ calls: calls.map((c) => ({ from: ME, to: c.to, data: c.data })) }] }, 'latest']);
  const bad = res[0].calls.findIndex((c) => c.status !== '0x1');
  if (bad > -1) die(`simulation of "${label}" failed at call ${bad + 1}: ${JSON.stringify(res[0].calls[bad].error || res[0].calls[bad].returnData).slice(0, 160)}`);
  log(`simulated "${label}" on live state: ${calls.length} call(s) ok`);
}

// ---- signing --------------------------------------------------------------------------------
const minimal = (n) => { const b = buf(BigInt(n).toString(16).padStart(64, '0')); let i = 0; while (i < b.length && b[i] === 0) i++; return b.subarray(i); };
function rlp(x) {
  const head = (len, off) => { if (len < 56) return Buffer.from([off + len]); const l = minimal(len); return Buffer.concat([Buffer.from([off + 55 + l.length]), l]); };
  if (Array.isArray(x)) { const body = Buffer.concat(x.map(rlp)); return Buffer.concat([head(body.length, 0xc0), body]); }
  const b = Buffer.isBuffer(x) ? x : minimal(x);
  if (b.length === 1 && b[0] < 0x80) return b;
  return Buffer.concat([head(b.length, 0x80), b]);
}
const DOMAIN_ORDER = [['name', 'string'], ['version', 'string'], ['chainId', 'uint256'], ['verifyingContract', 'address'], ['salt', 'bytes32']];
function signTyped(td) {
  const types = { ...td.types };
  if (!types.EIP712Domain) types.EIP712Domain = DOMAIN_ORDER.filter(([n]) => td.domain[n] !== undefined).map(([name, type]) => ({ name, type }));
  return signTypedDataWithKey({ types, primaryType: td.primaryType, domain: td.domain, message: td.message }, KEY);
}
function signAuthorization(a) {                       // EIP-7702: keccak(0x05 || rlp([chainId, address, nonce]))
  const digest = keccak256(Buffer.concat([Buffer.from([5]), rlp([BigInt(a.chainId), buf(a.address), BigInt(a.nonce)])]));
  const sig = signDigest(digest, KEY);
  if (recoverAddress(digest, sig).toLowerCase() !== ME) die('authorization self-check failed');
  return { chainId: a.chainId, address: a.address, nonce: a.nonce, r: '0x' + sig.slice(2, 66), s: '0x' + sig.slice(66, 130), yParity: parseInt(sig.slice(130, 132), 16) - 27 };
}

// ---- sponsor --------------------------------------------------------------------------------
const post = async (body, headers = {}) => { const r = await fetch(API, { method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(body) }); let j; try { j = await r.json(); } catch { j = {}; } return { status: r.status, body: j }; };
let feesPaid = 0n; const record = [], pending = [];      // pending: earlier steps not sent in a dry run, replayed first in simulations
async function sponsored(label, calls, check) {
  await simulate([...pending, ...calls], label);
  if (DRY && pending.length) { pending.push(...calls); record.push({ step: label, quote: 'simulated ok; quoted when the earlier steps are on chain', sent: false }); return; }
  const body = { sender: ME, calls: calls.map((c) => ({ to: c.to, data: c.data, value: '0' })) };
  let sp = await post(body);
  if (sp.status === 409) { body.redelegate = true; sp = await post(body); }
  if (sp.status !== 402) die(`sponsor did not quote "${label}": ${sp.status} ${JSON.stringify(sp.body).slice(0, 200)}`);
  const acc = sp.body.accepts[0], fee = BigInt(acc.amount);
  log(`quote for "${label}": ${usd(fee)} USDC`);
  if (fee > FEE_CAP || feesPaid + fee > FEE_TOTAL_CAP) die(`fee ${usd(fee)} is over the cap`);
  if (DRY) { pending.push(...calls); record.push({ step: label, quote: 'simulated ok; fee quote ' + usd(fee), sent: false }); return; }
  const nonce = hx(randomBytes(32)), validBefore = Math.floor(Date.now() / 1000) + 300;
  const paySig = signTyped({ domain: { name: acc.extra.name, version: acc.extra.version, chainId: acc.extra.chainId, verifyingContract: acc.extra.verifyingContract },
    types: { TransferWithAuthorization: [{ name: 'from', type: 'address' }, { name: 'to', type: 'address' }, { name: 'value', type: 'uint256' }, { name: 'validAfter', type: 'uint256' }, { name: 'validBefore', type: 'uint256' }, { name: 'nonce', type: 'bytes32' }] },
    primaryType: 'TransferWithAuthorization', message: { from: ME, to: acc.payTo, value: fee, validAfter: 0n, validBefore: BigInt(validBefore), nonce } });
  const payment = { x402Version: sp.body.x402Version || 2, scheme: acc.scheme, network: acc.network, payload: { signature: paySig, authorization: { from: ME, to: acc.payTo, value: acc.amount, validAfter: '0', validBefore: String(validBefore), nonce } } };
  sp = await post(body, { 'X-PAYMENT': Buffer.from(JSON.stringify(payment)).toString('base64') });
  if (sp.status !== 200) die(`sponsor refused the payment for "${label}": ${JSON.stringify(sp.body).slice(0, 300)}`);
  feesPaid += fee;
  const authorization = sp.body.authorization ? signAuthorization(sp.body.authorization) : null;
  const userOperation = { ...sp.body.userOperation, signature: signTyped(sp.body.typedData) };
  const ex = await post({ userOperation, authorization });
  if (ex.status !== 200 || ex.body.success === false) die(`"${label}" was paid for but did not execute: ${JSON.stringify(ex.body).slice(0, 300)}. Run the script again; finished steps are skipped.`);
  let block = null;
  for (let k = 0; k < 20 && ex.body.txHash; k++) { try { const rc = await rpc('eth_getTransactionReceipt', [ex.body.txHash]); block = parseInt(rc.blockNumber, 16); break; } catch { await sleep(1500); } }
  await sleep(2500);
  const result = await check();
  record.push({ step: label, tx: ex.body.txHash, block, check: result, fee: usd(fee), sent: true });
  log(`done "${label}": ${ex.body.txHash} | ${result}`);
}

// ---- the run --------------------------------------------------------------------------------
const J = (f) => JSON.parse(readFileSync(join(HERE, f), 'utf8'));
log('address', ME, DRY ? '(dry run: nothing is sent)' : '');
if (parseInt(await rpc('eth_chainId', []), 16) !== 8453) die('not Base mainnet');
let b = await balances();
log(`USDC ${usd(b.usdc)} | v1 coins ${usd(b.v1)} | v2 PRUSD ${usd(b.v2)} | ETH ${(Number(BigInt(await rpc('eth_getBalance', [ME, 'latest']))) / 1e18).toFixed(6)}`);
const needV2 = !(await hasCode(V2)), needRamp = !(await hasCode(RAMP));
const needMint = b.v1 > 0n || b.v2 === 0n;
let stepsLeft = BigInt([needV2, needMint, needRamp].filter(Boolean).length);
if (stepsLeft === 0n) { log('Everything is already on chain. Nothing to do.'); process.exit(0); }
if (b.usdc < FEE_RESERVE) die(`this address holds ${usd(b.usdc)} USDC; it needs at least ${usd(FEE_RESERVE)} to pay the first fee. Send USDC on Base to ${ME} and run again.`);

if (needV2) {
  await sponsored('deploy PRUSD v2', J('gas_req_prusd.json').calls, async () => ((await hasCode(V2)) ? 'code at ' + V2 : die('no code at the expected v2 address')));
  stepsLeft--; b = await balances();
} else log('PRUSD v2 already deployed at', V2);

if (needMint) {
  const budget = b.usdc + b.v1 - FEE_RESERVE * stepsLeft;          // v1 redeems 1:1 while its reserve covers its supply
  const mint = budget >= 400_000n ? 300_000n : (budget * 3n) / 4n, fund = budget >= 400_000n ? 100_000n : budget - mint;
  if (mint < 30_000n) die(`only ${usd(budget)} is left after fees; too little to mint. Send more USDC on Base to ${ME} and run again.`);
  const calls = [];
  if (b.v1 > 0n) calls.push({ to: V1, data: calldata('redeem(uint256)', [b.v1]) });
  calls.push({ to: USDC, data: calldata('approve(address,uint256)', [V2, fund + mint]) }, { to: V2, data: calldata('fund(uint256)', [fund]) }, { to: V2, data: calldata('mint(uint256)', [mint]) });
  log(`plan: ${b.v1 > 0n ? 'redeem ' + usd(b.v1) + ' of v1, ' : ''}fund ${usd(fund)}, mint ${usd(mint)}`);
  await sponsored('fund and mint v2', calls, async () => { const s = await call(V2, 'totalSupply()'), r = await call(V2, 'reserve()'), bp = await call(V2, 'bufferBps()'); return `v2 reserve ${usd(r)}, supply ${usd(s)}, buffer ${Number(bp) / 100}%`; });
  stepsLeft--;
} else log('v2 PRUSD already held:', usd(b.v2));

if (needRamp) {
  await sponsored('deploy escrow', J('gas_req_ramp.json').calls, async () => ((await hasCode(RAMP)) ? 'code at ' + RAMP : die('no code at the expected escrow address')));
} else log('escrow already deployed at', RAMP);

console.log('\nstep | tx | block | check | fee');
for (const r of record) console.log([r.step, r.tx || (r.sent ? '' : 'not sent'), r.block ?? '', r.check || r.quote || '', r.fee || ''].join(' | '));
if (!DRY) { writeFileSync(join(HERE, 'mainnet_run.json'), JSON.stringify({ address: ME, at: new Date().toISOString(), v2: V2, escrow: RAMP, fees: usd(feesPaid), steps: record }, null, 1)); log('saved script/mainnet_run.json | total fees', usd(feesPaid)); }
