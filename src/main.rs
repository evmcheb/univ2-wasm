// Only run this as a WASM if the export-abi feature is not set.
#![cfg_attr(not(feature = "export-abi"), no_main)]
extern crate alloc;

/// Initializes a custom, global allocator for Rust programs compiled to WASM.
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

mod erc20;

use crate::erc20::{UniswapV2ERC20, UniswapV2ERC20Params};

use alloy_primitives::{U32, Uint};
/// Import the Stylus SDK along with alloy primitive types for use in our program.
use stylus_sdk::{
    alloy_primitives::{U256, Address}, prelude::*,
    alloy_sol_types::sol,
    evm, block, msg, contract,
    call::RawCall, 
};

sol! {
    event Mint(address indexed sender, uint amount0, uint amount1);
    event Burn(address indexed sender, uint amount0, uint amount1, address indexed to);
    event Swap(
        address indexed sender,
        uint amount0In,
        uint amount1In,
        uint amount0Out,
        uint amount1Out,
        address indexed to
    );
    event Sync(uint112 reserve0, uint112 reserve1);
}

sol_interface! {
    interface IERC20 {
        function balanceOf(address owner) external view returns (uint);
    }
}

struct UniswapV2PairParams;

impl UniswapV2ERC20Params for UniswapV2PairParams {
}

const MINIMUM_LIQUIDITY: u64 = 1_000;
sol_storage! {
    #[entrypoint]
    struct UniswapV2Pair {
        address factory;
        address token0;
        address token1;

        uint112 reserve0;
        uint112 reserve1;
        uint32 blockTimestampLast;
        uint256 price0CumulativeLast;
        uint256 price1CumulativeLast;
        uint256 kLast;
        #[borrow]
        UniswapV2ERC20<UniswapV2PairParams> token;
    }
}

// External facing functions
#[external]
#[inherit(UniswapV2ERC20<UniswapV2PairParams>)]
impl UniswapV2Pair {
    pub fn initialize(&mut self, token0: Address, token1: Address) -> Result<(), Vec<u8>> {
        if self.factory.get() != Address::ZERO {
            return Err("Already initialized".into());
        }
        self.factory.set(msg::sender());
        self.token0.set(token0);
        self.token1.set(token1);
        Ok(())
    }

    pub fn mint(&mut self, to: Address) -> Result<U256, Vec<u8>> {
        let (_reserve0, _reserve1) = self.getReserves();
        let balance0 = IERC20::new(self.token0.get()).balanceOf(&*self, contract::address())?;
        let balance1 = IERC20::new(self.token1.get()).balanceOf(&*self, contract::address())?;
        let amount0 = balance0.checked_sub(_reserve0).ok_or("balance0-reserve0 overflow")?;
        let amount1 = balance1.checked_sub(_reserve1).ok_or("balance1-reserve1 overflow")?;

        // let fee_on = self._mintFee(_reserve0, _reserve1)?;
        let fee_on = false;
        let totalSupply = self.token.totalSupply()?;

        let liquidity = if totalSupply == U256::ZERO {
            self.token._mint(Address::ZERO, U256::from(MINIMUM_LIQUIDITY));
            self.sqrt(
                amount0.checked_mul(amount1).unwrap()
            ).checked_sub(U256::from(MINIMUM_LIQUIDITY)).ok_or("sqrt underflow")?
        } else {
            self.min(
                amount0.checked_mul(totalSupply).unwrap().checked_div(_reserve0).unwrap(),
                amount1.checked_mul(totalSupply).unwrap().checked_div(_reserve1).unwrap(),
            )
        };
        if liquidity == U256::ZERO {
            return Err("Liquidity is zero".into());
        }
        self.token._mint(to, liquidity);
        self._update(balance0, balance1, _reserve0, _reserve1);
        if fee_on {
            let r0 = U256::from(self.reserve0.get());
            let r1 = U256::from(self.reserve1.get());
            self.kLast.set(r0.checked_mul(r1).unwrap());
        }
        evm::log(
            Mint {
                sender: msg::sender(),
                amount0: amount0,
                amount1: amount1,
            }
        );
        Ok(liquidity)
    }

    pub fn burn(&mut self, to: Address) -> Result<(U256, U256), Vec<u8>> {
        let (_reserve0, _reserve1) = self.getReserves();
        let _token0 = self.token0.get();
        let _token1 = self.token1.get();
        let mut balance0 = IERC20::new(_token0).balanceOf(&*self, contract::address())?;
        let mut balance1 = IERC20::new(_token1).balanceOf(&*self, contract::address())?;
        let liquidity = self.token.balanceOf(contract::address())?;

        // let feeOn = self._mintFee(_reserve0, _reserve1)?;
        let feeOn = false;
        let _totalSupply = self.token.totalSupply()?;
        let mut amount0 = liquidity.checked_mul(balance0).unwrap() / _totalSupply;
        let mut amount1 = liquidity.checked_mul(balance1).unwrap() / _totalSupply;
        if amount0 == U256::ZERO || amount1 == U256::ZERO {
            return Err("INSUFFICIENT_LIQUIDITY_BURNED".into());
        }
        self.token._burn(contract::address(), liquidity);
        self._safeTransfer(_token0, to, amount0)?;
        self._safeTransfer(_token1, to, amount1)?;
        balance0 = IERC20::new(_token0).balanceOf(&*self, contract::address())?;
        balance1 = IERC20::new(_token1).balanceOf(&*self, contract::address())?;

        self._update(balance0, balance1, _reserve0, _reserve1);
        if feeOn {
            let r0 = U256::from(self.reserve0.get());
            let r1 = U256::from(self.reserve1.get());
            self.kLast.set(r0.checked_mul(r1).unwrap());
        }
        evm::log(
            Burn {
                sender: msg::sender(),
                amount0: amount0,
                amount1: amount1,
                to: to,
            }
        );
        Ok((amount0, amount1))
        
    }

    pub fn swap(
        &mut self,
        amount0_out: U256,
        amount1_out: U256,
        to: Address,
        data: Vec<u8>,
    ) -> Result<(), Vec<u8>> {
        if amount0_out == U256::ZERO || amount1_out == U256::ZERO {
            return Err("INSUFFICIENT_OUTPUT_AMOUNT".into());
        }
        let (_reserve0, _reserve1) = self.getReserves();
        if amount0_out >= _reserve0 || amount1_out >= _reserve1 {
            return Err("INSUFFICIENT_LIQUIDITY".into());
        }
        let token0 = IERC20::new(self.token0.get());
        let token1 = IERC20::new(self.token1.get());
        if amount0_out > U256::ZERO { self._safeTransfer(self.token0.get(), to, amount0_out)?; }
        if amount1_out > U256::ZERO { self._safeTransfer(self.token1.get(), to, amount1_out)?; }
        if !data.is_empty() {
            unsafe {RawCall::new().call(to, &data)?};
        }
        let balance0 = token0.balanceOf(&*self, contract::address())?;
        let balance1 = token1.balanceOf(&*self, contract::address())?;
        let amount0_in = balance0.saturating_sub(_reserve0.saturating_sub(amount0_out));
        let amount1_in = balance1.saturating_sub(_reserve1.saturating_sub(amount1_out));
        if amount0_in == U256::ZERO && amount1_in == U256::ZERO {
            return Err("INSUFFICIENT_INPUT_AMOUNT".into());
        }
        let balance0_adjusted = balance0.checked_mul(U256::from(1000)).unwrap()
            .checked_sub(amount0_in.checked_mul(U256::from(3)).unwrap())
            .ok_or("balance0Adjusted underflow")?;
        let balance1_adjusted = balance1.checked_mul(U256::from(1000)).unwrap()
            .checked_sub(amount1_in.checked_mul(U256::from(3)).unwrap())
            .ok_or("balance1Adjusted underflow")?;
        let k = _reserve0.checked_mul(_reserve1).unwrap().checked_mul(U256::from(1000)).unwrap();
        if balance0_adjusted.checked_mul(balance1_adjusted).unwrap() < k {
            return Err("K".into());
        }
        self._update(balance0, balance1, _reserve0, _reserve1);
        evm::log(
            Swap {
                sender: msg::sender(),
                amount0In: amount0_in,
                amount1In: amount1_in,
                amount0Out: amount0_out,
                amount1Out: amount1_out,
                to: to,
            }
        );

        Ok(())
    }

    pub fn skim(&mut self, to: Address) -> Result<(), Vec<u8>> {
        let token0 = IERC20::new(self.token0.get());
        let token1 = IERC20::new(self.token1.get());
        let balance0 = token0.balanceOf(&*self, contract::address())?;
        let balance1 = token1.balanceOf(&*self, contract::address())?;
        self._safeTransfer(self.token0.get(), to, balance0.checked_sub(U256::from(self.reserve0.get())).unwrap())?;
        self._safeTransfer(self.token1.get(), to, balance1.checked_sub(U256::from(self.reserve1.get())).unwrap())?;
        Ok(())
    }

    pub fn sync(&mut self) -> Result<(), Vec<u8>> {
        self._update(
            IERC20::new(self.token0.get()).balanceOf(&*self, contract::address())?,
            IERC20::new(self.token1.get()).balanceOf(&*self, contract::address())?,
            U256::from(self.reserve0.get()),
            U256::from(self.reserve1.get()),
        );
        Ok(())
    }
}

// Internal functions

impl UniswapV2Pair {
    pub fn _update(&mut self, balance0: U256, balance1: U256, reserve0: U256, reserve1: U256) {
        let block_timestamp = U32::from(block::timestamp() % 2e32 as u64);
        let time_elapsed = block_timestamp - self.blockTimestampLast.get();
        let Q112 = U256::from(2).pow(U256::from(112));

        if time_elapsed > U32::ZERO && reserve0 > U256::ZERO && reserve1 > U256::ZERO {
            let price0intial = self.price0CumulativeLast.get();
            let price1intial = self.price1CumulativeLast.get();

            let add0 = (U256::from(self.reserve1.get()) * Q112 / reserve0) * U256::from(time_elapsed);
            let add1 = (U256::from(self.reserve0.get()) * Q112 / reserve1) * U256::from(time_elapsed);
            self.price0CumulativeLast.set(price0intial + add0);
            self.price1CumulativeLast.set(price1intial + add1);
        }
        self.blockTimestampLast.set(block_timestamp);
        self.reserve0.set(Uint::<112, 2>::from(balance0));
        self.reserve1.set(Uint::<112, 2>::from(balance1));
        evm::log(
            Sync {
                reserve0: u128::from_be_bytes(self.reserve0.get().to_be_bytes()),
                reserve1: u128::from_be_bytes(self.reserve1.get().to_be_bytes())
            }
        );
    }

    fn min(&self, x: U256, y: U256) -> U256 {
        if x < y { x } else { y }
    }

    // babylonian method (https://en.wikipedia.org/wiki/Methods_of_computing_square_roots#Babylonian_method)
    fn sqrt(&self, y: U256) -> U256 {
        if y > U256::from(3) {
            let mut z = y;
            let mut x = y / U256::from(2) + U256::from(1);
            while x < z {
                z = x;
                x = (y / x + x) / U256::from(2);
            }
            z
        } else if y != U256::ZERO {
            U256::from(1)
        } else {
            U256::ZERO
        }
    }

    pub fn getReserves(&self) -> (U256, U256) {
        (U256::from(self.reserve0.get()), U256::from(self.reserve1.get()))
    }

    // Commented out to stay under 128kb uncompressed contract size
    /*
        pub fn _mintFee(&mut self, reserve0: U256, reserve1: U256) -> Result<bool, Vec<u8>> {
            // No fee switch
            let fee_on = false;
            let k_last = self.kLast.get();
            if fee_on {
                if k_last != U256::ZERO {
                    let root_k = self.sqrt(reserve0.checked_mul(reserve1).ok_or("Overflow")?);
                    let root_klast = self.sqrt(k_last);
                    if root_k > root_klast {
                        let numerator = self.token.totalSupply.get() * (root_k - root_klast);
                        let denominator = root_k * U256::from(5) + root_klast;
                        let liquidity = numerator / denominator;
                        if liquidity > U256::ZERO {
                            self.token._mint(msg::sender(), liquidity);
                            return Ok(true);
                        }
                    }
                }
            } else if k_last != U256::ZERO {
                self.kLast.set(U256::ZERO);
            }
            Ok(fee_on)
        }
    */

    pub fn _safeTransfer(&mut self, token: Address, to: Address, value: U256) -> Result<(), Vec<u8>> {
        let calldata: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];
        let ret = unsafe {
            RawCall::new().call(token, &calldata)
        };

        let success = match ret {
            Ok(_) => true,
            Err(_) => false,
        };
        let data = ret.unwrap_or_default();
        let is_true_bool = data.len() == 32 && data[31] == 1 && data[..31].iter().all(|&x| x == 0);
        if !(success && (data.len() == 0 || is_true_bool)) {
            return Err("UniswapV2: TRANSFER_FAILED".into());
        }

        Ok(())
    }

}

