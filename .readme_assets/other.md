## Roadmap
### `size` command
#### Goal
want to be able to quickly get correct size I need to use when opening a trade on a given ticker, based on expected volatility (in future based on pattern and my trading history with it too, but that's later).


#### Args
- coin (exchange doesn't matter, and we ignore liquidity for now, so neither does pair)
- --sl | -s
	% away: convert to exact price, print it (small reduction to possible human error)
	OR
	exact: print back % away (also to reduce possible human error)
	OR
	None: assume 20%


#### Steps
- [x] get total balance (today means bybit and binance, all margins (sapi and fapi))
- [x] get coin's price 

- [ ] risk est, mul the with default % of depo
  Large, requires a <plan>

#### Plan
- [x] 0.1: random criterion based on time it took to go same distance last time. 

- [ ] 1.0:
+ make a formula to quantify indirectional-vol
+ take entries from 3x back from the distance it last took to go the SL length
+ Exponentially weigh them, feed into da formula
+ trial and error the answer. Get any starting point, use the thing to trade, adjust as the intuition of this develops


#### Optimisations
- [ ] make all requests in one bunch, then after one comes through, check if we still need to await the rest.

#### Problems
- could have existing, correlatory with target, positions (ignore for now)

#### Blockers
- way of quantifying indirectional volatility
