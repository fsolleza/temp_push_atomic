
```
cargo run release 10000000 # write 10 million data items

# results look like this:

NONE
data 10000000
106.713867ms # time it took to write specified number of items
1220700000   # print sum to prevent compiler optimization

MUTEX         # mutex in the hot write path
data 10000000
954.218946ms
1290500000

SEQCST        # seqcst atomic in the hot write path
data 10000000
274.111292ms
1227500000

RELAXED       # relaxed atomic + a release mem fence in the hot write path
data 10000000
296.073475ms
1193700000

SEQCST WRITE+READ # seqcst atomic with specified readers (measured with 4)
data 10000000
3.724079091s
1212200000

FENCED WRITE+READ # relaxed atomic + a release memfence in the hot write path, acquire memfence in the read path
data 10000000
4.592099113s
1248500000
```
