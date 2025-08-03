use anchor_lang::prelude::*;

pub mod constants;
pub use constants::*;

pub mod error;
use crate::error::TradiumError;

pub mod instructions;
pub use instructions::*;

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
}
