use crate::constants::*;
use crate::error::TradiumError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token::{Mint, Token, TokenAccount};
use spl_token::instruction as spl_token_instruction;
use spl_token_2022::instruction as spl_token_2022_instruction;

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + Tradium::INIT_SPACE, // 8 bytes for discriminator + AnchorSize generated size
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
        space = 300, // Generous space for Token-2022 extensions (unchanged for vaults)
        seeds = [VAULT_SEED, pool.key().as_ref(), coin_mint.key().as_ref()],
        bump,
    )]
    /// CHECK: This account will be manually initialized as a token account
    pub coin_vault: UncheckedAccount<'info>,

    #[account(
        init,
        payer = payer,
        space = 300, // Generous space for Token-2022 extensions (unchanged for vaults)
        seeds = [VAULT_SEED, pool.key().as_ref(), pc_mint.key().as_ref()],
        bump,
    )]
    /// CHECK: This account will be manually initialized as a token account
    pub pc_vault: UncheckedAccount<'info>,

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
    _initial_coin_amount: u64, // Prefixed with underscore to indicate intentionally unused
    _initial_pc_amount: u64,   // Prefixed with underscore to indicate intentionally unused
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let coin_program_id = ctx.accounts.coin_token_program.key();
    let pc_program_id = ctx.accounts.pc_token_program.key();

    // Validate token programs
    if coin_program_id != SPL_TOKEN_PROGRAM_ID && coin_program_id != SPL_TOKEN_2022_PROGRAM_ID {
        return Err(TradiumError::InvalidTokenProgram.into());
    }
    if pc_program_id != SPL_TOKEN_PROGRAM_ID && pc_program_id != SPL_TOKEN_2022_PROGRAM_ID {
        return Err(TradiumError::InvalidTokenProgram.into());
    }

    // Access the bumps that Anchor automatically derives
    let pool_bump = ctx.bumps.pool;
    let _coin_vault_bump = ctx.bumps.coin_vault; // Prefixed with underscore
    let _pc_vault_bump = ctx.bumps.pc_vault; // Prefixed with underscore

    // Get keys before borrowing pool mutably
    let _pool_key = pool.key(); // Prefixed with underscore
    let coin_mint_key = ctx.accounts.coin_mint.key();
    let pc_mint_key = ctx.accounts.pc_mint.key();

    // Generate signer seeds for pool (authority for vaults)
    let pool_seeds = &[
        POOL_SEED,
        coin_mint_key.as_ref(),
        pc_mint_key.as_ref(),
        &[pool_bump],
    ];
    let pool_signer = &[&pool_seeds[..]];

    // FIXED: Use correct token program instruction based on coin mint type
    let init_coin_vault_ix = if coin_program_id == SPL_TOKEN_2022_PROGRAM_ID {
        // Use Token-2022 instruction for Token-2022 mints
        spl_token_2022_instruction::initialize_account(
            &coin_program_id,
            ctx.accounts.coin_vault.key,
            &ctx.accounts.coin_mint.key(),
            &pool.key(),
        )?
    } else {
        // Use standard SPL Token instruction for standard mints
        spl_token_instruction::initialize_account(
            &coin_program_id,
            ctx.accounts.coin_vault.key,
            &ctx.accounts.coin_mint.key(),
            &pool.key(),
        )?
    };

    // Initialize coin vault with correct instruction
    invoke_signed(
        &init_coin_vault_ix,
        &[
            ctx.accounts.coin_vault.to_account_info(),
            ctx.accounts.coin_mint.to_account_info(),
            pool.to_account_info(),
            ctx.accounts.rent.to_account_info(),
            ctx.accounts.coin_token_program.to_account_info(),
        ],
        pool_signer,
    )?;

    // FIXED: Use correct token program instruction based on pc mint type
    let init_pc_vault_ix = if pc_program_id == SPL_TOKEN_2022_PROGRAM_ID {
        // Use Token-2022 instruction for Token-2022 mints
        spl_token_2022_instruction::initialize_account(
            &pc_program_id,
            ctx.accounts.pc_vault.key,
            &ctx.accounts.pc_mint.key(),
            &pool.key(),
        )?
    } else {
        // Use standard SPL Token instruction for standard mints
        spl_token_instruction::initialize_account(
            &pc_program_id,
            ctx.accounts.pc_vault.key,
            &ctx.accounts.pc_mint.key(),
            &pool.key(),
        )?
    };

    // Initialize pc vault with correct instruction
    invoke_signed(
        &init_pc_vault_ix,
        &[
            ctx.accounts.pc_vault.to_account_info(),
            ctx.accounts.pc_mint.to_account_info(),
            pool.to_account_info(),
            ctx.accounts.rent.to_account_info(),
            ctx.accounts.pc_token_program.to_account_info(),
        ],
        pool_signer,
    )?;

    // Initialize the pool state
    pool.status = 1; // Active
    pool.nonce = [pool_bump];
    pool.coin_decimals = ctx.accounts.coin_mint.decimals as u64;
    pool.pc_decimals = ctx.accounts.pc_mint.decimals as u64;

    // Set mint and vault addresses
    pool.coin_vault_mint = ctx.accounts.coin_mint.key();
    pool.pc_vault_mint = ctx.accounts.pc_mint.key();
    pool.lp_mint = ctx.accounts.lp_mint.key();
    pool.coin_vault = ctx.accounts.coin_vault.key();
    pool.pc_vault = ctx.accounts.pc_vault.key();

    // Set the program IDs
    pool.coin_token_program = coin_program_id;
    pool.pc_token_program = pc_program_id;

    // Initialize fee with default values
    pool.fees.trade_fee_numerator = DEFAULT_TRADE_FEE;
    pool.fees.trade_fee_denominator = FEE_DENOMINATOR;
    pool.fees.swap_fee_numerator = DEFAULT_OWNER_FEE;
    pool.fees.swap_fee_denominator = FEE_DENOMINATOR;

    // Initialize whitelisted transfer hooks (empty by default)
    pool.whitelisted_transfer_hooks = [Pubkey::default(); crate::constants::MAX_WHITELISTED_HOOKS];
    pool.num_whitelisted_hooks = 0;

    // Set initialization flag
    pool.state_data.initialized = true;

    msg!("Pool initialized successfully");
    msg!("Coin mint: {}", ctx.accounts.coin_mint.key());
    msg!("PC mint: {}", ctx.accounts.pc_mint.key());
    msg!("LP mint: {}", ctx.accounts.lp_mint.key());
    msg!("Coin vault: {}", ctx.accounts.coin_vault.key());
    msg!("PC vault: {}", ctx.accounts.pc_vault.key());
    msg!("Allocated space: {} bytes", 8 + Tradium::INIT_SPACE);

    Ok(())
}
