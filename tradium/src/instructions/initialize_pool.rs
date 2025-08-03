use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use crate::state::*;
use crate::constants::*;
use crate::error::TradiumError;

#[derive(Accounts)]
#[instruction(bump: u8)]
pub struct InitializePool<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    
    #[account(
        init,
        payer = payer,
        space = 8 + std::mem::size_of::<Tradium>(),
        seeds = [POOL_SEED, coin_mint.key().as_ref(), pc_mint.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Tradium>,
    
    pub coin_mint: Account<'info, Mint>,
    pub pc_mint: Account<'info, Mint>,
    
    #[account(
        init,
        payer = payer,
        seeds = [LP_MINT_SEED, pool.key().as_ref()],
        bump,
        mint::decimals = 6,
        mint::authority = pool,
    )]
    pub lp_mint: Account<'info, Mint>,
    
    #[account(
        init,
        payer = payer,
        seeds = [VAULT_SEED, pool.key().as_ref(), coin_mint.key().as_ref()],
        bump,
        token::mint = coin_mint,
        token::authority = pool,
    )]
    pub coin_vault: Account<'info, TokenAccount>,
    
    #[account(
        init,
        payer = payer,
        seeds = [VAULT_SEED, pool.key().as_ref(), pc_mint.key().as_ref()],
        bump,
        token::mint = pc_mint,
        token::authority = pool,
    )]
    pub pc_vault: Account<'info, TokenAccount>,
    
    /// CHECK: This account will be validated in the instruction handler
    pub coin_token_program: UncheckedAccount<'info>,
    
    /// CHECK: This account will be validated in the instruction handler
    pub pc_token_program: UncheckedAccount<'info>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}