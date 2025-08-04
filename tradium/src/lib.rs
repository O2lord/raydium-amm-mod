use anchor_lang::prelude::*;

pub mod constants;
pub use constants::*;

pub mod error;
use crate::error::TradiumError;

pub mod instructions;
pub use instructions::*;

use crate::shared;

pub mod state;
pub use state::*;

declare_id!("9cmTderZ6Sthr4UYNvc3sdLyZDDw5fnzed2PtnyP4ZF7");

#[program]
pub mod tradium {
    use super::*;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        bump: u8,
        initial_coin_amount: u64,
        initial_pc_amount: u64,
    ) -> Result<()> {
        instructions::initialize_pool(ctx, bump, initial_coin_amount, initial_pc_amount)
    }

    pub fn deposit(ctx: Context<Deposit>, amount_coin: u64, amount_pc: u64) -> Result<()> {
        instructions::deposit(ctx, amount_coin, amount_pc)
    }

    pub fn withdraw(ctx: Context<Withdraw>, lp_amount: u64) -> Result<()> {
        instructions::withdraw(ctx, lp_amount)
    }

    pub fn swap(
        ctx: Context<Swap>,
        amount_in: u64,
        min_amount_out: u64,
        swap_direction: u8,
    ) -> Result<()> {
        instructions::swap(ctx, amount_in, min_amount_out, swap_direction)
    }
}
