
use core::marker::PhantomData;

/// Import the Stylus SDK along with alloy primitive types for use in our program.
use stylus_sdk::{
    alloy_primitives::{U256, Address, B256, U160}, prelude::*,
    alloy_sol_types::sol,
    evm, block, crypto, msg, contract
};

sol! {
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
}

pub trait UniswapV2ERC20Params {
}

sol_storage! {
    pub struct UniswapV2ERC20<T> {
        uint256 totalSupply;
        mapping(address => uint256) balances;
        mapping(address => mapping(address => uint256)) allowances;
        mapping(address => uint256) nonces;
        PhantomData<T> phantom;
    }
}

// External facing functions
#[external]
impl <T: UniswapV2ERC20Params>UniswapV2ERC20<T> {
    pub fn name(&self) -> Result<String, Vec<u8>> {
        Ok("Uniswap V2".to_string())
    }
    pub fn symbol(&self) -> Result<String, Vec<u8>> {
        Ok("UNI-V2".to_string())
    }
    pub fn decimals(&self) -> Result<u8, Vec<u8>> {
        Ok(18)
    }
    pub fn totalSupply(&self) -> Result<U256, Vec<u8>> {
        Ok(self.totalSupply.get())
    }
    pub fn balanceOf(&self, address: Address) -> Result<U256, Vec<u8>> {
        Ok(self.balances.get(address))
    }
    pub fn allowance(&self, owner: Address, spender: Address) -> Result<U256, Vec<u8>> {
        Ok(self.allowances.getter(owner).get(spender))
    }
    pub fn DOMAIN_SEPARATOR(&self) -> Result<Vec<u8>, Vec<u8>> {
        let domain_hash = crypto::keccak(
            "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
            .as_bytes(),
        );
        let name = crypto::keccak(self.name()?.as_bytes());
        let version = crypto::keccak("1".as_bytes());
        let chain_id = B256::from(U256::from(block::chainid()));
        let address: U160 = contract::address().into();
        let address = B256::from(address.to_be_bytes());

        // Concatenate
        Ok(crypto::keccak(
            &[
                domain_hash.0,
                name.0,
                version.0,
                chain_id.0,
                address.0,
            ]
            .concat(),
        ).to_vec())
    }

    // Commented out to stay under 128kb uncompressed contract size
    /*
        pub fn PERMIT_TYPEHASH(&self) -> Result<Vec<u8>, Vec<u8>> {
            Ok(crypto::keccak(
                "Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)"
                .as_bytes(),
            ).to_vec())
        }
    */

    pub fn approve(&mut self, spender: Address, value: U256) -> Result<bool, Vec<u8>> {
        self._approve(msg::sender(), spender, value);
        Ok(true)
    }

    pub fn transfer(&mut self, to: Address, value: U256) -> Result<bool, Vec<u8>> {
        self._transfer(msg::sender(), to, value)?;
        Ok(true)
    }

    pub fn transferFrom(&mut self, from: Address, to: Address, value: U256) -> Result<bool, Vec<u8>> {
        let mut from_allowance = self.allowances.setter(from);
        let mut allowance = from_allowance.setter(msg::sender());
        let old_allowance = allowance.get();
        if old_allowance < value {
            return Err("Insufficient allowance".to_string().into_bytes());
        }
        allowance.set(old_allowance - value);
        self._transfer(from, to, value)?;
        Ok(true)
    }

    // Commented out to stay under 128kb uncompressed contract size
    /*
    pub fn permit(
        &mut self,
        owner: Address,
        spender: Address,
        value: U256,
        deadline: U256,
        v: u8,
        r: Vec<u8>,
        s: Vec<u8>,
    ) -> Result<bool, Vec<u8>> {
        Err("No ecrecover in stylus yet".to_string().into_bytes())
    }
    */
}

// Internal functions

impl<T:UniswapV2ERC20Params> UniswapV2ERC20<T> {
    pub fn _mint(&mut self, to: Address, value: U256) {
        let mut balance = self.balances.setter(to);
        let new_balance = balance.get() + value;
        balance.set(new_balance);
        self.totalSupply.set(self.totalSupply.get() + value);
        evm::log(Transfer {
            from: Address::ZERO,
            to,
            value,
        });
    }
    pub fn _burn(&mut self, from: Address, value: U256) {
        let mut balance = self.balances.setter(from);
        let new_balance = balance.get() - value;
        balance.set(new_balance);
        self.totalSupply.set(self.totalSupply.get() - value);
        evm::log(Transfer {
            from,
            to: Address::ZERO,
            value,
        });
    }
    pub fn _approve(&mut self, owner: Address, spender: Address, value: U256) {
        let mut allowance = self.allowances.setter(owner);
        allowance.setter(spender).set(value);
        evm::log(Approval { owner, spender, value });
    }
    pub fn _transfer(&mut self, from: Address, to: Address, value: U256) -> Result<(), Vec<u8>> {
        let mut from_balance = self.balances.setter(from);
        let old_from_balance = from_balance.get();
        if old_from_balance < value {
            return Err("Insufficient balance".to_string().into_bytes());
        }
        from_balance.set(old_from_balance - value);
        let mut to_balance = self.balances.setter(to);
        let new_to_balance = to_balance.get() + value;
        to_balance.set(new_to_balance);
        evm::log(Transfer { from, to, value });
        Ok(())
    }
}

