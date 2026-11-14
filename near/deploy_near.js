// Creates a NEAR testnet account, deploys three test dollars and PRUSD, and runs the demo.
// Keys are written to KEYS (default ./.keys, git-ignored). Re-running resumes from state.json.
const { connect, keyStores, KeyPair } = require('near-api-js'); const fs = require('fs'), path = require('path');
const W = path.join(__dirname, 'target/wasm32-unknown-unknown/release/'), KEYS = process.env.KEYS || path.join(__dirname, '.keys');
const N = x => BigInt(Math.round(x * 1e6)) * 10n ** 18n, T = x => BigInt(x) * 10n ** 12n, ONE = 1n;
(async () => {
  fs.mkdirSync(KEYS, { recursive: true });
  const ks = new keyStores.UnencryptedFileSystemKeyStore(KEYS);
  const near = await connect({ networkId: 'testnet', nodeUrl: 'https://test.rpc.fastnear.com', keyStore: ks, helperUrl: 'https://helper.testnet.near.org' });
  const stateF = path.join(KEYS, 'state.json'); let st = fs.existsSync(stateF) ? JSON.parse(fs.readFileSync(stateF)) : {};
  const save = () => fs.writeFileSync(stateF, JSON.stringify(st, null, 1));
  if (!st.main) { const id = 'prusd-' + Date.now().toString(36) + '.testnet'; const kp = KeyPair.fromRandom('ed25519');
    await ks.setKey('testnet', id, kp); await near.createAccount(id, kp.getPublicKey()); st.main = id; st.tx = {}; st.rejected = {}; save(); }
  const main = await near.account(st.main); console.log('main', st.main, (await main.getAccountBalance()).available);
  const h = r => r.transaction.hash, legs = ['usd-a', 'usd-b', 'usd-c'].map(n => n + '.' + st.main), P = 'coin.' + st.main;
  for (const id of [...legs, P]) { if (st['acct_' + id]) continue; const coin = id === P;
    const kp = KeyPair.fromRandom('ed25519'); await ks.setKey('testnet', id, kp);
    await main.createAccount(id, kp.getPublicKey(), N(coin ? 2.6 : 2.1)); const a = await near.account(id);
    st.tx['deploy_' + id] = h(await a.deployContract(fs.readFileSync(W + (coin ? 'prusd' : 'tusd') + '.wasm')));
    await a.functionCall({ contractId: id, methodName: 'new', args: coin ? { legs } : { symbol: 'TEST-' + id.slice(4, 5).toUpperCase() }, gas: T(30) });
    st['acct_' + id] = true; save(); }
  st.legs = legs; st.prusd = P; save();
  const view = (c, m, args = {}) => main.viewFunction({ contractId: c, methodName: m, args });
  const call = async (k, c, m, args, dep = 0n, expectFail = false) => { if (st.tx[k] || st.rejected[k]) return;
    try { const r = await main.functionCall({ contractId: c, methodName: m, args, gas: T(150), attachedDeposit: dep });
      if (expectFail) throw new Error('UNEXPECTED SUCCESS ' + k); st.tx[k] = h(r); console.log('ok      ', k, st.tx[k]); }
    catch (e) { if (!expectFail || /UNEXPECTED/.test(e.message)) throw e; st.rejected[k] = (e.kind?.ExecutionError || e.message).slice(0, 140); console.log('rejected', k, '|', st.rejected[k]); }
    save(); };
  // 1. test dollars from the open faucets, deposited as credit
  for (const [i, L] of legs.entries()) { await call('faucet_' + i, L, 'faucet', { amount: '10000000' });
    await call('deposit_' + i, L, 'ft_transfer_call', { receiver_id: P, amount: '2000000', msg: '' }, ONE); }
  // 2. the mint gate: no buffer, no mint
  await call('mint_without_buffer', P, 'mint', { coins: '900000' }, 0n, true);
  // 3. fund the buffer, then mint
  await call('fund_0.60', P, 'fund', { amount: '600000' });
  await call('mint_0.90', P, 'mint', { coins: '900000' });
  // 4. dilution attack found by the search on Base: a mint larger than the buffer supports
  await call('mint_1.50_dilution', P, 'mint', { coins: '1500000' }, 0n, true);
  // 5. leg C "fails": redeem 0.30 at par taking only legs A and B
  const q = await view(P, 'quote_redeem', { coins: '300000', avoid_leg: 2 }); st.quote_avoiding_c = q; save();
  await call('redeem_0.30_avoiding_leg_c', P, 'redeem', { coins: '300000', take: q }, ONE);
  // 6. a run on the good legs: more than the pro-rata share of leg A
  await call('redeem_grab_leg_a', P, 'redeem', { coins: '300000', take: ['300000', '0', '0'] }, ONE, true);
  // 7. the buffer cannot leave while that would expose holders; a small part can
  await call('buffer_withdraw_all', P, 'withdraw_junior', { shares: '600000' }, ONE, true);
  await call('buffer_withdraw_0.09', P, 'withdraw_junior', { shares: '90000' }, ONE);
  st.state = await view(P, 'state'); st.balances = { prusd: await view(P, 'ft_balance_of', { account_id: st.main }), buffer_shares: await view(P, 'buffer_shares_of', { account_id: st.main }) };
  for (const [i, L] of legs.entries()) st.balances['test_' + 'abc'[i]] = await view(L, 'ft_balance_of', { account_id: st.main });
  save(); console.log(JSON.stringify(st, null, 1));
})().catch(e => { console.error('ERR', e.message || e); process.exit(1); });
