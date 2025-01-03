# rm_engine
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.85+-ab6000.svg)
[<img alt="crates.io" src="https://img.shields.io/crates/v/rm_engine.svg?color=fc8d62&logo=rust" height="20" style=flat-square>](https://crates.io/crates/rm_engine)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs&style=flat-square" height="20">](https://docs.rs/rm_engine)
![Lines Of Code](https://img.shields.io/badge/LoC-177-lightblue)
<br>
[<img alt="ci errors" src="https://img.shields.io/github/actions/workflow/status/valeratrades/rm_engine/errors.yml?branch=master&style=for-the-badge&style=flat-square&label=errors&labelColor=420d09" height="20">](https://github.com/valeratrades/rm_engine/actions?query=branch%3Amaster) <!--NB: Won't find it if repo is private-->
[<img alt="ci warnings" src="https://img.shields.io/github/actions/workflow/status/valeratrades/rm_engine/warnings.yml?branch=master&style=for-the-badge&style=flat-square&label=warnings&labelColor=d16002" height="20">](https://github.com/valeratrades/rm_engine/actions?query=branch%3Amaster) <!--NB: Won't find it if repo is private-->

<!-- markdownlint-disable -->
<details>
  <summary>
    <h2>Installation</h2>
  </summary>
	<pre><code class="language-sh">TODO</code></pre>
</details>
<!-- markdownlint-restore -->

## Usage
TODO


## Roadmap


### `size` command
#### Goal
want to be able to quickly get correct size I need to use when opening a trade on a given ticker, based on expected volatility (in future based on pattern and my trading history with it too, but that's later).


### Args
- coin (exchange doesn't matter, and we ignore liquidity for now, so neither does pair)
- --sl | -s
	% away: convert to exact price, print it (small reduction to possible human error)
	OR
	exact: print back % away (also to reduce possible human error)
	OR
	None: assume 20%


### Steps
- get total balance (today means bybit and binance, all margins (sapi and fapi))
- get coin's price
- request 500 klines on 1d, then going down, timeframes; until we hit 1m, or have isolated at least 10 candles back from now, that together make up the distance of over 1 stop-loss. Then somehow adjust risk estimate based on that time (do whatever to start with, you'll spend hours deciding otherwise)
    funky: could take this diff only in the trade's direction, or make necessary previous move in opposite direction larger, to account for half indirectional volatility, half encouraging following momentum

### Problems
- the closer the stop-loss, the higher the risk. Really should be adjusting for that
- could have existing, correlatory with target, positions (ignore for now)

<br>

<sup>
This repository follows <a href="https://github.com/valeratrades/.github/tree/master/best_practices">my best practices</a>.
</sup>

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
