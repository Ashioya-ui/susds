#!/usr/bin/env python3
"""Run the v2 deployment, the migration and every Ramp path on live Base state with
eth_simulateV1. Real bytecode, real USDC, the holder's real balance; nothing is broadcast."""
import json, os, requests, sys, time
D = os.path.dirname(os.path.abspath(__file__)); RPC = "https://mainnet.base.org"
USDC = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"; V1 = "0x48d55161357bd8782833ff80bf33e5e30846aa30"
V2 = open(D + "/PRUSD_ADDRESS").read().strip().lower(); H = "0xd1da74fda379cb622305c9ed04a96e3432c9998f"
X = "0x00000000000000000000000000000000000b0b01"      # a second party: the buyer
DEPLOYER = "0x4e59b44847b379578588920ca78fbf26c0b4956c"; MAX = 2**256 - 1
ramp = json.load(open(D + "/ramp.json")); M = {k.split("(")[0]: v for k, v in ramp["methods"].items()}
M.update(approve="095ea7b3", transfer="a9059cbb", balanceOf="70a08231", totalSupply="18160ddd", reserve="cd3293de",
         mint="a0712d68", redeem="db006a75", fund="ca1d209d")
w = lambda v: (int(v, 16) if isinstance(v, str) else v).to_bytes(32, "big").hex()
cd = lambda f, *a: "0x" + M[f] + "".join(w(x) for x in a)
c = lambda f, to, d: {"from": f, "to": to, "data": d}
def rpc(m, p):
    for k in range(8):
        r = requests.post(RPC, json={"jsonrpc": "2.0", "id": 1, "method": m, "params": p}, timeout=40).json()
        if "result" in r: return r["result"]
        time.sleep(1 + k)
    sys.exit(str(r))
blk = rpc("eth_getBlockByNumber", ["latest", False]); tag = blk["number"]; t0 = int(blk["timestamp"], 16)
SALT = b"PRUSD:Ramp:v1".ljust(32, b"\0").hex()
ramp_init = "0x" + SALT + ramp["bytecode"][2:] + w(V2) + w(USDC)
dep = json.load(open(D + "/gas_req_prusd.json"))["calls"][0]
migrate = [c(H, V1, cd("redeem", 300000)), c(H, USDC, cd("approve", V2, 400000)), c(H, V2, cd("fund", 100000)), c(H, V2, cd("mint", 300000))]
setup = [c(H, dep["to"], dep["data"])] + migrate + [c(H, DEPLOYER, ramp_init)]
RAMP = "0x" + rpc("eth_simulateV1", [{"blockStateCalls": [{"calls": setup}]}, tag])[0]["calls"][-1]["returnData"][-40:]
PH = "11" * 32; RH = "22" * 32
common = setup + [c(H, V2, cd("approve", RAMP, MAX)), c(H, RAMP, cd("postBond", 100000)), c(H, V2, cd("transfer", X, 100000)),
                  c(X, V2, cd("approve", RAMP, MAX)), c(X, RAMP, "0x" + M["open"] + w(H) + w(50000) + w(6) + PH), c(H, RAMP, cd("accept", 0))]
reads = [c(H, V2, cd("balanceOf", H)), c(H, V2, cd("balanceOf", X)), c(H, V2, cd("balanceOf", RAMP)), c(H, RAMP, cd("bond", H)),
         c(H, RAMP, cd("reserved", H)), c(H, V2, "0x" + M["reserve"]), c(H, V2, "0x" + M["totalSupply"]), c(H, V1, "0x" + M["totalSupply"])]
receipt = c(H, RAMP, "0x" + M["submitReceipt"] + w(0) + RH)
def run(name, now, later, expect, must_fail=()):
    blocks = [{"calls": common + now}]
    if later is not None: blocks.append({"blockOverrides": {"time": hex(t0 + 7300)}, "calls": later})
    blocks[-1]["calls"] = blocks[-1]["calls"] + reads
    res = rpc("eth_simulateV1", [{"blockStateCalls": blocks}, tag])
    calls = [x for b in res for x in b["calls"]]; body = calls[:-8]; got = [int(x["returnData"], 16) for x in calls[-8:]]
    acts = len(common) + len(now) + len(later or [])
    st = [x["status"] == "0x1" for x in body]
    bad = [i for i, s in enumerate(st) if s == ((i - len(common)) in must_fail)]
    ok = got == expect and not bad
    print(f"{'PASS' if ok else 'FAIL'}  {name:34s} holder,buyer,ramp,bond,reserved,v2reserve,v2supply,v1supply = {got}" + (f"  unexpected status at {bad}" if bad else ""))
    return ok
S = c(X, RAMP, cd("release", 0)); stranger = "0x00000000000000000000000000000000000e0e02"
r = [
 run("release", [receipt, c(X, RAMP, cd("release", 0))], None, [150000, 50000, 100000, 100000, 0, 400000, 300000, 0]),
 run("dispute -> PRUSD buffer", [receipt, c(X, RAMP, cd("dispute", 0))], None, [100000, 0, 50000, 50000, 0, 400000, 150000, 0]),
 run("refund after timeout (+1% of bond)", [], [c(stranger, RAMP, cd("refund", 0))], [100000, 100500, 99500, 99500, 0, 400000, 300000, 0]),
 run("claim after silent buyer", [receipt], [c(H, RAMP, cd("claim", 0))], [150000, 50000, 100000, 100000, 0, 400000, 300000, 0]),
 run("cancel before accept is closed off", [c(X, RAMP, cd("cancel", 0)), c(H, RAMP, cd("withdrawBond", 50001)), c(stranger, RAMP, cd("release", 0)),
      c(stranger, RAMP, cd("refund", 0)), c(H, RAMP, cd("claim", 0)), c(H, RAMP, cd("withdrawBond", 50000))], None,
      [150000, 0, 150000, 50000, 50000, 400000, 300000, 0], must_fail=(0, 1, 2, 3, 4)),
 run("late receipt and late dispute fail", [], [receipt, c(X, RAMP, cd("dispute", 0)), c(X, RAMP, cd("release", 0)), c(X, RAMP, cd("release", 0))], [150000, 50000, 100000, 100000, 0, 400000, 300000, 0], must_fail=(0, 1, 3)),
]
print(json.dumps(dict(block=int(tag, 16), v2=V2, ramp=RAMP, all_pass=all(r))))
json.dump({"sender": H, "calls": [{"to": x["to"], "data": x["data"], "value": "0"} for x in migrate]}, open(D + "/gas_req_migrate.json", "w"))
json.dump({"sender": H, "calls": [{"to": DEPLOYER, "data": ramp_init, "value": "0"}]}, open(D + "/gas_req_ramp.json", "w"))
open(D + "/RAMP_ADDRESS", "w").write(RAMP)
