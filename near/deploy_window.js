// Deploys The Window on NEAR testnet over three existing test dollars, funds it, replays the
// three scares at 1:3333 scale, and deletes the contract's keys. Keys go to KEYS (git-ignored).
const { connect, keyStores, KeyPair } = require('near-api-js'); const fs = require('fs'), path = require('path');
const KEYS = process.env.KEYS || path.join(__dirname, '.keys-window'), WASM = path.join(__dirname, 'target/wasm32-unknown-unknown/release/window.wasm');
const LEGS = ['usd-a', 'usd-b', 'usd-c'].map(n => n + '.prusd-mwgt5ryd.testnet'), NM = ['USDC', 'USDT', 'DAI'];
const N = x => BigInt(Math.round(x * 1e6)) * 10n ** 18n, T = x => BigInt(x) * 10n ** 12n;
(async () => {
  fs.mkdirSync(KEYS, { recursive: true }); const ks = new keyStores.UnencryptedFileSystemKeyStore(KEYS);
  const near = await connect({ networkId: 'testnet', nodeUrl: 'https://test.rpc.fastnear.com', keyStore: ks, helperUrl: 'https://helper.testnet.near.org' });
  const f = path.join(KEYS, 'state.json'); let st = fs.existsSync(f) ? JSON.parse(fs.readFileSync(f)) : {}; const save = () => fs.writeFileSync(f, JSON.stringify(st, null, 1));
  if (!st.main) { const id = 'window-' + Date.now().toString(36) + '.testnet', kp = KeyPair.fromRandom('ed25519'); await ks.setKey('testnet', id, kp); await near.createAccount(id, kp.getPublicKey()); st.main = id; st.tx = {}; save(); }
  const main = await near.account(st.main), P = 'pool.' + st.main; st.pool = P;
  if (!st.deployed) { const kp = KeyPair.fromRandom('ed25519'); await ks.setKey('testnet', P, kp); await main.createAccount(P, kp.getPublicKey(), N(3)); const a = await near.account(P);
    st.tx.deploy = (await a.deployContract(fs.readFileSync(WASM))).transaction.hash;
    await a.functionCall({ contractId: P, methodName: 'new', args: { legs: LEGS, d0_bps: '0', dmax_bps: '500', cap_div: '6' }, gas: T(30) }); st.deployed = true; save(); }
  const view = (m, args = {}) => main.viewFunction({ contractId: P, methodName: m, args });
  const call = async (k, c, m, args, dep = 0n) => { if (st.tx[k]) return; st.tx[k] = (await main.functionCall({ contractId: c, methodName: m, args, gas: T(150), attachedDeposit: dep })).transaction.hash; save(); console.log('ok', k, st.tx[k]); };
  const sell = (k, i, j, usd) => call(k, LEGS[i], 'ft_transfer_call', { receiver_id: P, amount: String(Math.round(usd * 1e6)), msg: `sell:${j}:0` }, 1n);
  for (const [i, L] of LEGS.entries()) { await call('faucet_' + i, L, 'faucet', { amount: '1000000000' }); await call('credit_' + i, L, 'ft_transfer_call', { receiver_id: P, amount: '300000000', msg: '' }, 1n); }
  await call('deposit_900', P, 'deposit', { shares: '900000000' });
  const snap = async k => { const s = await view('state'); st[k] = { discount_bps: s.discount_now_bps, used: s.window_used, holdings: s.holdings, share_ppm: s.par_value_per_share_ppm, fees: s.fees_earned }; save(); };
  await sell('jun2023_usdt_9', 1, 2, 9); await snap('after_jun2023');
  await sell('nov2022_usdt_51_more', 1, 2, 51); await snap('after_nov2022');
  await sell('mar2023_usdc_150', 0, 2, 150); await snap('after_mar2023');
  st.quote_usdc_when_full = await view('quote_sell', { leg: 0, amount: '1000000' });
  await sell('recovery_dai_for_usdt_30', 2, 1, 30); await snap('after_unwind');
  if (process.argv[2] === 'lock' && !st.tx.delete_key) { const a = await near.account(P); for (const k of await a.getAccessKeys()) st.tx.delete_key = (await a.deleteKey(k.public_key)).transaction.hash; }
  st.state = await view('state'); save(); console.log(JSON.stringify(st, null, 1));
})().catch(e => { console.error('ERR', e.message || e); process.exit(1); });
