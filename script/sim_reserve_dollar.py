#!/usr/bin/env python3
"""ReserveDollar on live Base state with real USDC and USDT (eth_simulateV1; nothing is broadcast).
Actor A is an address that holds USDC; a large USDT holder sends it USDT inside the simulation."""
import json, os, sys, time, requests
D = os.path.dirname(os.path.abspath(__file__)); RPC = "https://mainnet.base.org"
USDC = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"; USDT = "0xfde4c96c8593536e31f229ea8f37b2ada2699bb2"
A = "0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb"; TW = sys.argv[1].lower(); DEPLOYER = "0x4e59b44847b379578588920ca78fbf26c0b4956c"
art = json.load(open(D + "/reserve_dollar.json")); M = {k.split("(")[0]: v for k, v in art["methods"].items()}
w = lambda v: (int(v, 16) if isinstance(v, str) else v).to_bytes(32, "big").hex()
c = lambda f, to, d: {"from": f, "to": to, "data": d}
def rpc(m, p):
    for k in range(8):
        r = requests.post(RPC, json={"jsonrpc": "2.0", "id": 1, "method": m, "params": p}, timeout=40).json()
        if "result" in r: return r["result"]
        time.sleep(1 + k)
    sys.exit(str(r))
arr = lambda xs: w(len(xs)) + "".join(w(x) for x in xs)
def init(d_bps, salt):
    n = 2; args = w(0x60) + w(0x60 + 32 * (n + 1)) + w(0x60 + 64 * (n + 1)) + arr([USDC, USDT]) + arr([6, 6]) + arr(d_bps)
    return "0x" + salt.encode().ljust(32, b"\0").hex() + art["bytecode"][2:] + args
def run(title, d_bps, steps):
    data = init(d_bps, "PRRD:" + title[:20])
    pre = [c(TW, USDT, "0xa9059cbb" + w(A) + w(50_000_000)), c(A, DEPLOYER, data)]
    R = "0x" + rpc("eth_simulateV1", [{"blockStateCalls": [{"calls": pre}]}, "latest"])[0]["calls"][-1]["returnData"][-40:]
    calls = pre + [c(A, USDC, "0x095ea7b3" + w(R) + w(2**256 - 1)), c(A, USDT, "0x095ea7b3" + w(R) + w(2**256 - 1))]
    names = []
    for name, m, args, expect in steps:
        if m == "redeem": data = "0x" + M[m] + w(args[0]) + w(0x40) + arr(args[1])
        else: data = "0x" + M[m] + "".join(w(a) for a in args)
        calls.append(c(A, R, data)); names.append((name, expect))
    views = ["holdings", "totalSupply", "bufferShares", "safe"]
    calls += [c(A, R, "0x" + M[v]) for v in views]
    res = rpc("eth_simulateV1", [{"blockStateCalls": [{"calls": calls}]}, "latest"])[0]["calls"]
    body = res[4:4 + len(names)]; ok_all = all(x["status"] == "0x1" for x in res[:4])
    print(f"\n{title}  (contract {R}, d = {[x/100 for x in d_bps]}%)")
    for (name, expect), r in zip(names, body):
        got = r["status"] == "0x1"; ok_all &= (got == expect)
        print(f"  {'PASS' if got == expect else 'FAIL'}  {name}: {'accepted' if got else 'rejected'}")
    hv = res[-4]["returnData"]; hold = [int(hv[2 + 64 * (2 + k): 2 + 64 * (3 + k)], 16) for k in range(2)]
    print(f"  end: holdings USDC {hold[0]/1e6:.4f}, USDT {hold[1]/1e6:.4f} | supply {int(res[-3]['returnData'],16)/1e6:.4f} | buffer shares {int(res[-2]['returnData'],16)/1e6:.4f} | SAFE {int(res[-1]['returnData'],16)==1}")
    return ok_all
a = run("par by choice", [10_000, 10_000], [
    ("mint $2 with no buffer", "mint", [2_000_000], False),
    ("fund the buffer with $2", "fund", [2_000_000], True),
    ("mint $2", "mint", [2_000_000], True),
    ("mint $2 more (dilution)", "mint", [2_000_000], False),
    ("USDT fails: redeem $1 taking only USDC", "redeem", [1_000_000, [1_000_000, 0]], True),
    ("grab more than the share of USDC", "redeem", [500_000, [500_000 + 2_000, 0]], False),
    ("withdraw the whole buffer", "withdrawBuffer", [2_000_000], False),
    ("withdraw a little buffer", "withdrawBuffer", [1_000], True)])
b = run("loss bound 20% per leg", [2_000, 2_000], [
    ("mint $2 with no buffer", "mint", [2_000_000], False),
    ("fund the buffer with $0.2224 (11.1%)", "fund", [222_400], True),
    ("mint $2", "mint", [2_000_000], True),
    ("mint $2 more (dilution)", "mint", [2_000_000], False),
    ("redeem $1 in equal parts", "redeem", [1_000_000, [500_000, 500_000]], True),
    ("withdraw the whole buffer", "withdrawBuffer", [222_400], False)])
print("\nall as expected:", a and b)
