#!/usr/bin/env python3
"""Open-ended adversarial search against the Pro Rata Dollar contract on Base mainnet.

Every candidate is a sequence of real contract calls executed by eth_simulateV1 on the
current Base state: the deployed bytecode, the real USDC contract, real balances.
Nothing is broadcast and no state is overridden. The search keeps an archive of
behaviours (which calls succeed, who gains) and never terminates on its own: run it
again and it resumes from archive.json on the newest block.

usage: evolve.py v1|v2 [generations] [population]
"""
import json, random, sys, time, os, requests

RPCS = ["https://mainnet.base.org", "https://base.drpc.org", "https://1rpc.io/base",
        "https://base-rpc.publicnode.com"]
USDC = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"
V1 = "0x48d55161357bd8782833ff80bf33e5e30846aa30"
V2 = open(os.path.join(os.path.dirname(__file__), "../script/PRUSD_ADDRESS")).read().strip().lower()
H = "0xd1da74fda379cb622305c9ed04a96e3432c9998f"      # the real holder
WHALES = ["0x3304e22ddaa22bcdc5fca2269b418046ae7b566a", "0xf977814e90da44bfa03b6295a0616a897441acec",
          "0x0b0a5886664376f59c351ba3f598c8a8b4d0a6f3", "0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb"]
MAX = 2**256 - 1
SEL = dict(mint="a0712d68", redeem="db006a75", fund="ca1d209d", reserve="cd3293de", supply="18160ddd",
           bal="70a08231", transfer="a9059cbb", approve="095ea7b3", tfrom="23b872dd")
w = lambda v: (int(v, 16) if isinstance(v, str) else v).to_bytes(32, "big").hex()
cd = lambda s, *a: "0x" + SEL[s] + "".join(w(x) for x in a)

_i = 0
def rpc(method, params, batch=None):
    global _i
    body = batch if batch is not None else {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    for attempt in range(12):
        url = RPCS[_i % len(RPCS)]
        try:
            r = requests.post(url, json=body, timeout=40).json()
            if batch is not None:
                if isinstance(r, list) and all("result" in x for x in r):
                    return [x["result"] for x in sorted(r, key=lambda x: x["id"])]
            elif "result" in r:
                return r["result"]
        except Exception:
            pass
        _i += 1; time.sleep(0.4 * (attempt + 1))
    raise SystemExit("rpc failed: " + method)

def call(to, data, block="latest"):
    return int(rpc("eth_call", [{"to": to, "data": data}, block]) or "0x0", 16)

OPS = ["mint", "redeem", "fund", "donate", "give_H", "give_V", "give_self", "pull_H_coin",
       "approve_H", "pull_reserve", "pull_H_usdc", "VICTIM"]

def tx(op, x, A, V, hbal):
    c = lambda f, to, d: {"from": f, "to": to, "data": d}
    return {"mint": c(A, V, cd("mint", x)), "redeem": c(A, V, cd("redeem", x)), "fund": c(A, V, cd("fund", x)),
            "donate": c(A, USDC, cd("transfer", V, x)), "give_H": c(A, V, cd("transfer", H, x)),
            "give_V": c(A, V, cd("transfer", V, x)), "give_self": c(A, V, cd("transfer", A, x)),
            "pull_H_coin": c(A, V, cd("tfrom", H, A, x)), "approve_H": c(A, V, cd("approve", H, x)),
            "pull_reserve": c(A, USDC, cd("tfrom", V, A, x)), "pull_H_usdc": c(A, USDC, cd("tfrom", H, A, x)),
            "VICTIM": c(H, V, cd("redeem", hbal))}[op]

def reads(A, V):
    c = lambda to, d: {"from": A, "to": to, "data": d}
    return [c(USDC, cd("bal", A)), c(USDC, cd("bal", H)), c(V, cd("bal", A)), c(V, cd("bal", H)),
            c(V, "0x" + SEL["reserve"]), c(V, "0x" + SEL["supply"])]

def worth(usdc, coin, r, s):
    return usdc + (coin * min(r, s) // s if s else 0)

def main():
    target = sys.argv[1]; gens = int(sys.argv[2]) if len(sys.argv) > 2 else 30
    pop_n = int(sys.argv[3]) if len(sys.argv) > 3 else 24
    V = V1 if target == "v1" else V2
    block = rpc("eth_blockNumber", [])
    wb = [(call(USDC, cd("bal", a), block), a) for a in WHALES]
    AB, A = max(wb)
    setup = []
    if target == "v2":      # the real deployment request, then the holder's real history replayed with real USDC
        dep = json.load(open(os.path.join(os.path.dirname(__file__), "../script/gas_req_prusd.json")))["calls"][0]
        setup = [{"from": H, "to": dep["to"], "data": dep["data"]},
                 {"from": H, "to": USDC, "data": cd("approve", V, MAX)},
                 {"from": H, "to": V, "data": cd("fund", 100000)}, {"from": H, "to": V, "data": cd("mint", 300000)}]
    setup.append({"from": A, "to": USDC, "data": cd("approve", V, MAX)})
    base = rpc("eth_simulateV1", [{"blockStateCalls": [{"calls": setup + reads(A, V)}]}, block])[0]["calls"]
    assert all(c["status"] == "0x1" for c in base), "setup failed"
    _, _, _, hbal, R, S = [int(c["returnData"], 16) for c in base[-6:]]
    pool = [0, 1, 2, hbal - 1, hbal, hbal + 1, R, R + 1, S, S + 1, 10**6, 10**9, 10**12, AB, AB + 1, 2**255, MAX]
    amt = lambda: random.choice(pool) if random.random() < .7 else random.randrange(1, max(2, min(AB, 10**13)))
    gene = lambda: (random.choice(OPS[:-1]), amt())
    def fresh():
        g = [gene() for _ in range(random.randint(1, 8))]; g.insert(random.randint(0, len(g)), ("VICTIM", 0)); return g
    def mutate(g):
        g = list(g); m = random.random()
        if m < .3 and len(g) < 14: g.insert(random.randint(0, len(g)), gene())
        elif m < .5 and len(g) > 2:
            k = random.randrange(len(g)); g = g if g[k][0] == "VICTIM" else g[:k] + g[k+1:]
        elif m < .7: a, b = random.randrange(len(g)), random.randrange(len(g)); g[a], g[b] = g[b], g[a]
        else:
            k = random.randrange(len(g))
            if g[k][0] != "VICTIM": g[k] = (g[k][0], amt()) if random.random() < .6 else gene()
        return g
    def cross(a, b):
        a2 = [x for x in a if x[0] != "VICTIM"]; b2 = [x for x in b if x[0] != "VICTIM"]
        g = (a2[:random.randint(0, len(a2))] + b2[random.randint(0, len(b2)):])[:13] or [gene()]
        g.insert(random.randint(0, len(g)), ("VICTIM", 0)); return g
    def evaluate(pop):
        out = []
        for k in range(0, len(pop), 6):
            chunk = pop[k:k+6]
            batch = [{"jsonrpc": "2.0", "id": i, "method": "eth_simulateV1", "params": [{"blockStateCalls": [{"calls":
                      setup + reads(A, V) + [tx(o, x, A, V, hbal) for o, x in g] + reads(A, V)}]}, block]}
                     for i, g in enumerate(chunk)]
            for g, res in zip(chunk, rpc(None, None, batch)):
                cs = res[0]["calls"][len(setup):]
                p = [int(c["returnData"], 16) for c in cs[:6]]; q = [int(c["returnData"], 16) for c in cs[-6:]]
                mid = cs[6:-6]; ok = [c["status"] == "0x1" for c in mid]
                vi = [o for o, _ in g].index("VICTIM")
                paid = int(mid[vi]["returnData"], 16) if ok[vi] else 0
                profit = worth(q[0], q[2], q[4], q[5]) - worth(p[0], p[2], p[4], p[5])
                harm = hbal - paid                       # shortfall of the holder's redemption against par
                b0 = p[4] * 10**6 // p[5]; b1 = q[4] * 10**6 // q[5] if q[5] else 10**6
                bmin = b1
                out.append(dict(genome=g, ok=ok, profit=profit, harm=harm, victim_ok=ok[vi], paid=paid, B0=b0, B1=b1, R1=q[4], S1=q[5],
                                score=max(profit, harm), sig="".join(o[0] + ("+" if s else "-") for (o, _), s in zip(g, ok))
                                + "|" + ("P" if profit > 0 else "p") + ("H" if harm > 0 else "h") + ("B" if b1 < 10**6 else "b")))
        return out
    path = os.path.join(os.path.dirname(__file__), f"archive_{target}.json")
    archive = json.load(open(path)) if os.path.exists(path) else {}
    random.seed(int(block, 16))
    pop = [fresh() for _ in range(pop_n)]; n = 0
    for gen in range(gens):
        for e in evaluate(pop):
            n += 1; e["block"] = int(block, 16)
            old = archive.get(e["sig"])
            if old is None or (e["score"], -e["B1"]) > (old["score"], -old["B1"]): archive[e["sig"]] = e
        elite = sorted(archive.values(), key=lambda e: (e["score"], -e["B1"]), reverse=True)
        par = [e["genome"] for e in elite[:8]] + [e["genome"] for e in random.sample(elite, min(8, len(elite)))]
        pop = [mutate(random.choice(par)) if random.random() < .7 else cross(random.choice(par), random.choice(par))
               for _ in range(pop_n - 4)] + [fresh() for _ in range(4)]
        json.dump(archive, open(path, "w"))
        best = elite[0]
        print(f"gen {gen:3d} sims {n:5d} behaviours {len(archive):4d} max_profit {max(e['profit'] for e in archive.values())} "
              f"max_harm {max(e['harm'] for e in archive.values())} min_B_ppm {min(e['B1'] for e in archive.values())}", flush=True)
    print(json.dumps(dict(target=target, contract=V, block=int(block, 16), attacker=A, attacker_usdc=AB, holder=H,
                          holder_coins=hbal, reserve=R, supply=S, sims=n, behaviours=len(archive))))
if __name__ == "__main__":
    main()
