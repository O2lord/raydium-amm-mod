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

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
