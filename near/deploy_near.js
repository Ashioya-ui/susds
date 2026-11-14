const { connect, keyStores, KeyPair } = require('near-api-js'); const fs = require('fs');
const W = '/home/claude/susds/near/target/wasm32-unknown-unknown/release/';
const N = x => BigInt(Math.round(x * 1e6)) * 10n ** 18n, T = x => BigInt(x) * 10n ** 12n, ONE = 1n;
(async () => {
  const ks = new keyStores.UnencryptedFileSystemKeyStore('/home/claude/keys-near');
  const near = await connect({ networkId: 'testnet', nodeUrl: 'https://test.rpc.fastnear.com', keyStore: ks, helperUrl: 'https://helper.testnet.near.org' });
  const stateF = '/home/claude/keys-near/state.json'; let st = fs.existsSync(stateF) ? JSON.parse(fs.readFileSync(stateF)) : {};
  const save = () => fs.writeFileSync(stateF, JSON.stringify(st, null, 1));
  if (!st.main) { const id = 'proratadollar-' + Date.now().toString(36) + '.testnet'; const kp = KeyPair.fromRandom('ed25519');
    await ks.setKey('testnet', id, kp); await near.createAccount(id, kp.getPublicKey()); st.main = id; st.tx = {}; save(); }
  const main = await near.account(st.main); console.log('main', st.main, (await main.getAccountBalance()).available);
  const h = r => r.transaction.hash;
  for (const n of ['tusd', 'prusd']) { const id = n + '.' + st.main; if (st[n]) continue;
    const kp = KeyPair.fromRandom('ed25519'); await ks.setKey('testnet', id, kp);
    await main.createAccount(id, kp.getPublicKey(), N(2.5)); const a = await near.account(id);
    st.tx['deploy_' + n] = h(await a.deployContract(fs.readFileSync(W + n + '.wasm')));
    st.tx['init_' + n] = h(await a.functionCall({ contractId: id, methodName: 'new', args: n == 'prusd' ? { reserve_token: 'tusd.' + st.main } : {}, gas: T(30) }));
    st[n] = id; save(); }
  const call = async (k, c, m, args, dep = 0n, gas = 100) => { if (st.tx[k]) return; const r = await main.functionCall({ contractId: c, methodName: m, args, gas: T(gas), attachedDeposit: dep }); st.tx[k] = h(r); save();
    console.log(k, st.tx[k], JSON.stringify(r.status)); };
  const view = (c, m, args = {}) => main.viewFunction({ contractId: c, methodName: m, args });
  await call('faucet', st.tusd, 'faucet', { amount: '1000000' });
  await call('fund', st.tusd, 'ft_transfer_call', { receiver_id: st.prusd, amount: '100000', msg: 'fund' }, ONE);
  await call('mint', st.tusd, 'ft_transfer_call', { receiver_id: st.prusd, amount: '300000', msg: 'mint' }, ONE);
  await call('redeem', st.prusd, 'redeem', { coins: '50000' }, ONE);
  const out = {}; for (const m of ['reserve', 'ft_total_supply', 'backing_ppm', 'buffer_bps', 'ft_metadata']) out[m] = await view(st.prusd, m);
  out.prusd_balance = await view(st.prusd, 'ft_balance_of', { account_id: st.main }); out.tusd_balance = await view(st.tusd, 'ft_balance_of', { account_id: st.main });
  out.tusd_held_by_prusd = await view(st.tusd, 'ft_balance_of', { account_id: st.prusd });
  console.log(JSON.stringify({ ...st, views: out }, null, 1));
})().catch(e => { console.error('ERR', e.message || e); process.exit(1); });
