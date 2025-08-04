use crate::constants::*;
use crate::error::TradiumError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

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

pub fn initialize_pool(
    ctx: Context<InitializePool>,
    bump: u8,
    initial_coin_amount: u64,
    initial_pc_amount: u64,
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let coin_program_id = ctx.accounts.coin_token_program.key();
    // Validate token programs
    let coin_program_id = ctx.accounts.coin_token_program.key();
    let pc_program_id = ctx.accounts.pc_token_program.key();

    // Check if the provided token programs are valid
    if coin_program_id != SPL_TOKEN_PROGRAM_ID && coin_program_id != SPL_TOKEN_2022_PROGRAM_ID {
        return Err(TradiumError::InvalidTokenProgram.into());
    }
    if pc_program_id != SPL_TOKEN_PROGRAM_ID && pc_program_id != SPL_TOKEN_2022_PROGRAM_ID {
        return Err(TradiumError::InvalidTokenProgram.into());
    }
    // Initialize the pool state
    pool.status = 1; // Active
    pool.nonce = [bump];
    pool.coin_decimals = ctx.accounts.coin_mint.decimals as u64;
    pool.pc_decimals = ctx.accounts.pc_mint.decimals as u64;

    //set mint and vault addresses
    pool.coin_vault_mint = ctx.accounts.coin_mint.key();
    pool.pc_vault_mint = ctx.accounts.pc_mint.key();
    pool.lp_mint = ctx.accounts.lp_mint.key();
    pool.coin_vault = ctx.accounts.coin_vault.key();
    pool.pc_vault = ctx.accounts.pc_vault.key();

    //set the program IDs
    pool.coin_token_program = coin_program_id;
    pool.pc_token_program = pc_program_id;
    pool.pc_token_program = ctx.accounts.token_program.key();

    //Initialize fee with default values
    pool.fees.trade_fee_numerator = DEFAULT_TRADE_FEE;
    pool.fees.trade_fee_denominator = FEE_DENOMINATOR;
    pool.fees.swap_fee_numerator = DEFAULT_OWNER_FEE;
    pool.fees.swap_fee_denominator = FEE_DENOMINATOR;

    // Initialize whitelisted transfer hooks (empty by default)
    pool.whitelisted_transfer_hooks = [Pubkey::default(); crate::constants::MAX_WHITELISTED_HOOKS];
    pool.num_whitelisted_hooks = 0;

    //set initialization flag
    pool.state_data.initialized = true;

    msg!("Pool initialized successfully");
    msg!("Coin mint: {}", ctx.accounts.coin_mint.key());
    msg!("PC mint: {}", ctx.accounts.pc_mint.key());
    msg!("LP mint: {}", ctx.accounts.lp_mint.key());

    Ok(())
}
