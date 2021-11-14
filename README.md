
```
# Run with cargo run --release
# len set to 10_000_000

NONE
data 10000000
9.086392ms
1284400000

MUTEX
data 10000000
157.457687ms    // ~15x baseline
1309200000

SEQCST
data 10000000
92.514267ms     // ~10x baseline
1348100000

RELAXED
data 10000000
92.45782ms      // ~10x baseline
1318300000

ATOMIC BOOL ARRAY
data 10000000
80.119395ms     // ~9x baseline
1313200000

MUTEX WRITE+READ
data 10000000
7.174818961s    // ~45x uncontended mutex
1253100000

SEQCST WRITE+READ
data 10000000
3.884809001s    // ~42x uncontended SeqCst Atomic
1144400000

FENCED WRITE+READ
data 10000000
4.241647548s    // ~46x uncontended Release fence + Relaxed atomic
1127600000

ATOMIC BOOL WRITE+READ
data 10000000
86.12531ms      // ~1x uncontended atomic bool array
1257100000
```
