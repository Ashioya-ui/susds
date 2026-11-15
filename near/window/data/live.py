#!/usr/bin/env python3
"""Shadow run of The Window on the live market.

Every hour: read Curve's 3pool on Ethereum mainnet (real USDC, USDT, DAI), then apply the
Window's rule to a shadow pool: holders sell a coin to the window while its marginal penalty
is below the discount the market shows for that coin. No money moves; the inputs are real.
State and log: docs/shadow.json.

  live.py check          the Python port reproduces the Rust contract's replay on data/3pool.json
  live.py backfill N     start a new shadow pool N hours ago and run it up to now
  live.py step           one step at the latest block (run hourly by GitHub Actions)
"""
import json, os, sys, time, requests
HERE = os.path.dirname(os.path.abspath(__file__)); OUT = os.path.join(HERE, "../../../docs/shadow.json")
RPCS = ["https://eth-mainnet.public.blastapi.io", "https://eth.drpc.org", "https://rpc.mevblocker.io", "https://ethereum-rpc.publicnode.com"]
POOL = "0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7"; NM = ["USDC", "USDT", "DAI"]
D0, DMAX, CAP_DIV, START = 0, 500, 6, 3_000_000_000_000
w = lambda v: hex(v)[2:].rjust(64, "0")
_i = 0
def rpc(body):
    global _i
    for k in range(12):
        try:
            r = requests.post(RPCS[_i % len(RPCS)], json=body, timeout=30).json()
            if isinstance(body, list):
                if isinstance(r, list) and all(x.get("result") not in (None, "0x") for x in r): return [x["result"] for x in sorted(r, key=lambda x: x["id"])]
            elif r.get("result") is not None: return r["result"]
        except Exception: pass
        _i += 1; time.sleep(0.4 + 0.4 * k)
    sys.exit("rpc failed")
def point(tag):
    dy = lambda i, j, dec: "0x5e0d443f" + w(i) + w(j) + w(100_000 * 10**dec)
    calls = ["0x4903b0d1" + w(0), "0x4903b0d1" + w(1), "0x4903b0d1" + w(2), dy(1, 2, 6), dy(0, 2, 18)]
    body = [{"jsonrpc": "2.0", "id": q, "method": "eth_call", "params": [{"to": POOL, "data": d}, tag]} for q, d in enumerate(calls)]
    body.append({"jsonrpc": "2.0", "id": 9, "method": "eth_getBlockByNumber", "params": [tag, False]})
    r = rpc(body); blk = r[5]
    dai, usdc, usdt = int(r[0], 16) // 10**12, int(r[1], 16), int(r[2], 16)
    return {"block": int(blk["number"], 16), "time": int(blk["timestamp"], 16), "bal": [usdc, usdt, dai], "v": [int(r[3], 16) / 1e11, 1.0, int(r[4], 16) / 1e11]}

# ---- the Window's rule: an exact integer port of near/window/src/core.rs -------------------
def excess(h, s, i): return max(0, h[i] - s // 3)
def swap(st, i, j, a):
    h, s = st["h"], st["s"]; k = s // CAP_DIV; r = s // 3
    e0 = max(0, h[i] - r); e1 = max(0, h[i] + a - r)
    if a == 0 or e1 > k or i == j: return None
    den = 20_000 * k
    fee = (D0 * (e1 - e0) * 2 * k + (DMAX - D0) * (e1 * e1 - e0 * e0) + den - 1) // den
    if fee >= a or a - fee > h[j]: return None
    h[i] += a; h[j] -= a - fee; st["fees"] += fee
    return fee
def step(st, v):
    top = max(v); price = [x / top for x in v]; k = st["s"] // CAP_DIV; sold = [0, 0, 0]
    for i in range(3):
        bps = int((1.0 - price[i]) * 10_000.0)
        target = min(k, k * bps // DMAX); e = excess(st["h"], st["s"], i)
        if target > e:
            for j in sorted([x for x in range(3) if x != i], key=lambda x: -price[x]):
                if swap(st, i, j, target - e) is not None: sold[i] = target - e; break
    return price, sold
def fresh(): return {"h": [START // 3] * 3, "s": START, "fees": 0}

def check():
    d = json.load(open(os.path.join(HERE, "3pool.json"))); want = [(1942, -0.74), (25000, -11.08), (61, -0.12)]
    for sc, (fee, low) in zip(d["scares"], want):
        st = fresh(); lo = 0.0
        for p in sc["points"]:
            price, _ = step(st, [p["usdc_in_usdt"], 1.0, p["dai_in_usdt"]])
            lo = min(lo, sum(st["h"][i] * price[i] for i in range(3)) / st["s"] - 1)
        got = (round(st["fees"] / 1e6), round(lo * 100, 2)); print(sc["name"], "penalties $%d, lowest %+.2f%%" % got, "OK" if got == (fee, low) else "MISMATCH")
        assert got == (fee, low)
def record(doc, p):
    st = doc["state"]; price, sold = step(st, p["v"]); tot = sum(p["bal"]); k = st["s"] // CAP_DIV
    doc["log"].append({"time": p["time"], "block": p["block"], "share_pct": [round(100 * b / tot, 2) for b in p["bal"]], "price": [round(x, 5) for x in price],
                       "sold_usd": [round(x / 1e6) for x in sold], "window_used_pct": [round(100 * excess(st["h"], st["s"], i) / k, 2) for i in range(3)],
                       "penalties_usd": round(st["fees"] / 1e6, 2), "value_at_market_pct": round((sum(st["h"][i] * price[i] for i in range(3)) / st["s"] - 1) * 100, 4),
                       "value_at_par_pct": round((sum(st["h"]) / st["s"] - 1) * 100, 4)})
    doc["log"] = doc["log"][-720:]; doc["updated"] = p["time"]
def save(doc): json.dump(doc, open(OUT, "w"), separators=(",", ":"))
if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "step"
    if cmd == "check": check()
    elif cmd == "backfill":
        n = int(sys.argv[2]); tip = point("latest")
        doc = {"what": "Shadow run of The Window on live Curve 3pool data (Ethereum mainnet). Inputs are real; the pool is hypothetical; no money moves.",
               "source": "Curve 3pool " + POOL, "coins": NM, "params": {"pool_usd": START // 10**6, "d0_bps": D0, "dmax_bps": DMAX, "cap_div": CAP_DIV}, "state": fresh(), "log": []}
        for hr in range(n, 0, -1):
            record(doc, point(hex(tip["block"] - hr * 300))); time.sleep(0.1)
        record(doc, tip); doc["started"] = doc["log"][0]["time"]; save(doc); print("points", len(doc["log"]), json.dumps(doc["log"][-1]))
    else:
        doc = json.load(open(OUT)); p = point("latest")
        if p["block"] > doc["log"][-1]["block"]: record(doc, p); save(doc)
        print(json.dumps(doc["log"][-1]))
