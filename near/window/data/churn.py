#!/usr/bin/env python3
"""Proposition 9 (bounded churn) against real data: the hourly composition of Curve's 3pool
during three scares, read as a path of reserve weights (USDC, USDT, DAI).
For each scare: how much the real pool moved, and how much a reserve following the same
targets would have moved if it only took steps that keep its path self-approaching (rule (1))."""
import json, math, os
d = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "3pool.json")))
dist = lambda a, b: math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))
l1 = lambda a, b: sum(abs(x - y) for x, y in zip(a, b))
def ok(path, c):                      # adding c keeps the path self-approaching: distance to c never increases along it
    return all(dist(path[j], c) <= dist(path[i], c) + 1e-15 for i in range(len(path)) for j in range(i + 1, len(path)))
print("scare | hours | real pool: volume moved | real path self-approaching? | leash: steps taken | leash: volume moved | bound of Prop 9 | volume avoided")
for sc in d["scares"]:
    W = [[b / sum(p["bal"]) for b in (p["bal"][1], p["bal"][2], p["bal"][0])] for p in sc["points"]]
    real = sum(l1(W[t], W[t + 1]) for t in range(len(W) - 1))
    viol = sum(1 for k in range(2, len(W)) if not ok(W[:k], W[k]))
    path = [W[0]]
    for w in W[1:]:
        if ok(path, w): path.append(w)
    vol = sum(l1(path[t], path[t + 1]) for t in range(len(path) - 1))
    diam = max(dist(p, path[0]) for p in path); end = dist(path[-1], path[0])
    bound = math.sqrt(2) * min(3.1297 * diam, 5.3332 * end)
    assert vol <= bound + 1e-12
    print(f"{sc['name']} | {len(W)-1} | {real*100:.1f}% of pool | {'yes' if viol == 0 else 'no, ' + str(viol) + ' hours break it'} | {len(path)-1} | {vol*100:.1f}% of pool | {bound*100:.1f}% | {(1-vol/real)*100:.0f}%")
