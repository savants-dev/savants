#!/usr/bin/env python3
"""Compare two Claude Code benchmark runs and calculate SEQ."""

import json
import sys

def main():
    if len(sys.argv) < 3:
        print("Usage: python3 compare.py <without-savants.json> <with-savants.json>")
        sys.exit(1)

    with open(sys.argv[1]) as f:
        r1 = json.loads(f.read())
    with open(sys.argv[2]) as f:
        r2 = json.loads(f.read())

    u1 = r1.get("usage", {})
    u2 = r2.get("usage", {})
    t1 = u1.get("input_tokens", 0) + u1.get("output_tokens", 0)
    t2 = u2.get("input_tokens", 0) + u2.get("output_tokens", 0)
    c1 = r1.get("cost_usd", 0)
    c2 = r2.get("cost_usd", 0)
    n1 = r1.get("num_turns", 0)
    n2 = r2.get("num_turns", 0)
    d1 = r1.get("duration_ms", 0)
    d2 = r2.get("duration_ms", 0)

    saved = max(0, t1 - t2)
    pct = int(saved / t1 * 100) if t1 > 0 else 0
    time_saved = max(0, d1 - d2)
    speedup = round(d1 / d2, 1) if d2 > 0 else 0

    # SEQ calculation
    precision = min(40, 35 if t2 < t1 else 15)
    cost_eff = min(35, int(35 * saved / max(1, t1)))
    velocity = 25 if d2 < d1 else 15
    seq = precision + cost_eff + velocity
    label = (
        "Exceptional" if seq >= 90 else
        "Excellent" if seq >= 75 else
        "Good" if seq >= 60 else
        "Moderate" if seq >= 40 else
        "Baseline"
    )

    print()
    print("════════════════════════════════════════════════")
    print("  WITHOUT SAVANTS")
    print("════════════════════════════════════════════════")
    print(f"  Answer:  {r1.get('result', '')[:200]}")
    print(f"  Tokens:  {t1:,}")
    print(f"  Turns:   {n1}")
    print(f"  Time:    {d1:,}ms ({d1/1000:.1f}s)")
    print(f"  Cost:    ${c1:.4f}")
    print()
    print("════════════════════════════════════════════════")
    print("  WITH SAVANTS")
    print("════════════════════════════════════════════════")
    print(f"  Answer:  {r2.get('result', '')[:200]}")
    print(f"  Tokens:  {t2:,}")
    print(f"  Turns:   {n2}")
    print(f"  Time:    {d2:,}ms ({d2/1000:.1f}s)")
    print(f"  Cost:    ${c2:.4f}")
    print()
    print("╔══════════════════════════════════════════════════╗")
    print("║        SAVANTS EFFICIENCY QUOTIENT (SEQ)         ║")
    print("╠══════════════════════════════════════════════════╣")
    print(f"║  Score:    {seq}/100 ({label})")
    print(f"║")
    print(f"║  Tokens:   {t1:,} → {t2:,} ({pct}% reduction)")
    print(f"║  Turns:    {n1} → {n2}")
    print(f"║  Time:     {d1/1000:.1f}s → {d2/1000:.1f}s ({speedup}x faster)")
    print(f"║  Cost:     ${c1:.4f} → ${c2:.4f}")
    print(f"║")
    print(f"║  Precision:       {precision}/40")
    print(f"║  Cost efficiency: {cost_eff}/35")
    print(f"║  Velocity:        {velocity}/25")
    print("╚══════════════════════════════════════════════════╝")
    print()

    # Machine-readable output
    result = {
        "without_savants": {"tokens": t1, "turns": n1, "duration_ms": d1, "cost_usd": c1},
        "with_savants": {"tokens": t2, "turns": n2, "duration_ms": d2, "cost_usd": c2},
        "seq": {"score": seq, "label": label, "precision": precision, "cost_efficiency": cost_eff, "velocity": velocity},
        "improvement": {"token_reduction_pct": pct, "speedup_x": speedup, "tokens_saved": saved, "time_saved_ms": time_saved},
    }
    with open("/tmp/benchmark-result.json", "w") as f:
        json.dump(result, f, indent=2)
    print(f"Results saved to /tmp/benchmark-result.json")

if __name__ == "__main__":
    main()
