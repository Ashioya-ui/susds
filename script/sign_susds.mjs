#!/usr/bin/env node
// Sign the sUSDs deployment on Base mainnet with the core keystore; print the raw transaction.
// Nothing is sent: broadcast the output with eth_sendRawTransaction. Env: NONCE (default 0).
import { readFileSync } from 'node:fs';
import { scryptSync, createDecipheriv } from 'node:crypto';
import { keccak256, signDigest, addressOfKey, encodeTypes } from './evm.mjs';
const buf = (h) => Buffer.from(String(h).replace(/^0x/, ''), 'hex'), hx = (b) => `0x${Buffer.from(b).toString('hex')}`;
const minimal = (n) => { const b = buf(BigInt(n).toString(16).padStart(64, '0')); let i = 0; while (i < b.length && b[i] === 0) i++; return b.subarray(i); };
function rlp(x) {
  const head = (len, off) => { if (len < 56) return Buffer.from([off + len]); const l = minimal(len); return Buffer.concat([Buffer.from([off + 55 + l.length]), l]); };
  if (Array.isArray(x)) { const body = Buffer.concat(x.map(rlp)); return Buffer.concat([head(body.length, 0xc0), body]); }
  const b = Buffer.isBuffer(x) ? x : minimal(x);
  if (b.length === 1 && b[0] < 0x80) return b;
  return Buffer.concat([head(b.length, 0x80), b]);
}
const ks = JSON.parse(readFileSync('/home/claude/spectrum/audit/wallets/core', 'utf8')).crypto, p = ks.kdfparams, ct = buf(ks.ciphertext);
const raw = readFileSync('/home/claude/spectrum/audit/wallets/core.pw', 'utf8');
let key;
for (const pw of [raw, raw.replace(/\r?\n$/, '')]) {
  const dk = scryptSync(Buffer.from(pw), buf(p.salt), p.dklen, { N: p.n, r: p.r, p: p.p, maxmem: 2 ** 28 });
  if (hx(keccak256(Buffer.concat([dk.subarray(16, 32), ct]))) !== `0x${ks.mac.replace(/^0x/, '')}`) continue;
  const d = createDecipheriv('aes-128-ctr', dk.subarray(0, 16), buf(ks.cipherparams.iv));
  key = hx(Buffer.concat([d.update(ct), d.final()]));
}
if (!key) throw new Error('keystore');
const USDC = '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913';           // USDC on Base mainnet
const art = JSON.parse(readFileSync(new URL('./susds.json', import.meta.url), 'utf8')).contracts['src/SUSDs.sol:SUSDs'];
const data = buf(art.bin + Buffer.from(encodeTypes(['address'], [USDC])).toString('hex'));
const nonce = BigInt(process.env.NONCE || 0), chainId = 8453n, tip = 1_000_000n, maxFee = 50_000_000n, gas = 1_200_000n;
const fields = [chainId, nonce, tip, maxFee, gas, Buffer.alloc(0), 0n, data, []];
const digest = keccak256(Buffer.concat([Buffer.from([2]), rlp(fields)]));
const sig = signDigest(digest, key);
const { r, s, v } = typeof sig === 'string' ? { r: buf(sig.slice(2, 66)), s: buf(sig.slice(66, 130)), v: BigInt(parseInt(sig.slice(130, 132), 16)) } : sig;
const yParity = (v === 27n || v === 28n) ? v - 27n : BigInt(v) & 1n;
const rawTx = hx(Buffer.concat([Buffer.from([2]), rlp([...fields, yParity, buf(hx(r)), buf(hx(s))])]));
const from = addressOfKey(key);
const addr = hx(keccak256(rlp([buf(from), nonce]))).slice(26);
console.log(JSON.stringify({ from, nonce: Number(nonce), contract: `0x${addr}`, gas: Number(gas), maxFeeWei: Number(maxFee), rawTx }, null, 1));
