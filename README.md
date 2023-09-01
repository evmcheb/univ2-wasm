# Uniswap V2 implementation in WASM

A PoC Uniswap V2 implementation for Arbitrum Stylus.

The main features are implmeneted: provide/remove liquidity and swap.
- The liq tokens don't support `permit` due to lack of `ecrecover` in Stylus (at the time of writing).- `UniswapV2Pair` is implemented (no Factory pattern)
- No fee-switch support

WASM smart contracts are very cool, checkout the [Arbitrum Stylus docs](https://github.com/OffchainLabs/stylus)

# Contact

[@evmcheb](https://twitter.com/evmcheb)