#!/usr/bin/env python3
"""Hourly state of Curve's 3pool (DAI, USDC, USDT) on Ethereum mainnet during three scares,
read from archive nodes: balances and the pool's own exchange rates for a $100,000 trade.
Writes data/3pool.json. No keys needed."""
import json, time, datetime, requests, os
RPCS = ["https://eth-mainnet.public.blastapi.io", "https://eth.drpc.org", "https://rpc.mevblocker.io"]
POOL = "0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7"
DEC = [18, 6, 6]; NM = ["DAI", "USDC", "USDT"]
w = lambda v: hex(v)[2:].rjust(64, "0")
_i = 0
def rpc(body):
    global _i
    for k in range(12):
        try:
            r = requests.post(RPCS[_i % len(RPCS)], json=body, timeout=30).json()
            if isinstance(body, list):
                if isinstance(r, list) and all("result" in x and x["result"] not in (None, "0x") for x in r):
                    return [x["result"] for x in sorted(r, key=lambda x: x["id"])]
            elif r.get("result") is not None:
                return r["result"]
        except Exception:
            pass
        _i += 1; time.sleep(0.5 + 0.5 * k)
    raise SystemExit("rpc failed")
def ts(n): return int(rpc({"jsonrpc": "2.0", "id": 1, "method": "eth_getBlockByNumber", "params": [hex(n), False]})["timestamp"], 16)
def block_at(t):
    lo, hi = 15_600_000, int(rpc({"jsonrpc": "2.0", "id": 1, "method": "eth_blockNumber", "params": []}), 16)
    while lo < hi:
        m = (lo + hi) // 2
        if ts(m) < t: lo = m + 1
        else: hi = m
    return lo
def dy(i, j): return "0x5e0d443f" + w(i) + w(j) + w(100_000 * 10**DEC[i])
def point(n):
    calls = [("0x4903b0d1" + w(k)) for k in range(3)] + [dy(1, 2), dy(0, 2), dy(2, 1)]
    body = [{"jsonrpc": "2.0", "id": q, "method": "eth_call", "params": [{"to": POOL, "data": d}, hex(n)]} for q, d in enumerate(calls)]
    r = [int(x, 16) for x in rpc(body)]
    bal = [r[k] // 10**(DEC[k] - 6) if DEC[k] > 6 else r[k] for k in range(3)]       # 6-decimal dollars
    usdc_in_usdt, dai_in_usdt, usdt_in_usdc = r[3] / 1e11, r[4] / 1e11, r[5] / 1e11   # per $1, for a $100k trade, after the pool fee
    return bal, [dai_in_usdt, usdc_in_usdt, usdt_in_usdc]
U = lambda s: int(datetime.datetime.strptime(s, "%Y-%m-%d %H:%M").replace(tzinfo=datetime.timezone.utc).timestamp())
SCARES = [("Nov 2022 (FTX)", "2022-11-08 00:00", "2022-11-12 00:00"), ("Mar 2023 (SVB)", "2023-03-10 12:00", "2023-03-14 00:00"), ("Jun 2023", "2023-06-14 12:00", "2023-06-16 12:00")]
out = []
for name, a, b in SCARES:
    n0 = block_at(U(a)); hours = (U(b) - U(a)) // 3600; pts = []
    for h in range(hours + 1):
        n = n0 + h * 300
        bal, rate = point(n)
        pts.append({"block": n, "bal": bal, "dai_in_usdt": rate[0], "usdc_in_usdt": rate[1], "usdt_in_usdc": rate[2]})
        time.sleep(0.12)
    pts[0]["time"] = ts(pts[0]["block"]); pts[-1]["time"] = ts(pts[-1]["block"])
    out.append({"name": name, "from": a, "to": b, "points": pts})
    lo = lambda key: min(pts, key=lambda p: p[key])
    print(name, "points", len(pts), "| min USDC/USDT", round(lo("usdc_in_usdt")["usdc_in_usdt"], 4), "| min DAI/USDT", round(lo("dai_in_usdt")["dai_in_usdt"], 4), "| min USDT/USDC", round(lo("usdt_in_usdc")["usdt_in_usdc"], 4),
          "| max USDT share", round(max(p["bal"][2] / sum(p["bal"]) for p in pts) * 100, 1), "| min USDT share", round(min(p["bal"][2] / sum(p["bal"]) for p in pts) * 100, 1), flush=True)
json.dump({"source": "Curve 3pool " + POOL + " on Ethereum mainnet, eth_call at historical blocks", "rate_trade_size_usd": 100000, "coins": NM, "scares": out}, open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "3pool.json"), "w"))
