use crate::error::TradiumError;
use crate::shared; // Import shared module
use crate::state::Tradium;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};
use anchor_spl::token_interface::{
    Mint as MintInterface, TokenAccount as TokenAccountInterface, TokenInterface,
};
use spl_token_2022::extension::transfer_hook::TransferHook;
use spl_token_2022::extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub pool: Account<'info, Tradium>,

    #[account(mut)]
    pub user_coin_account: InterfaceAccount<'info, TokenAccountInterface>,

    #[account(mut)]
    pub user_pc_account: InterfaceAccount<'info, TokenAccountInterface>,

    #[account(mut)]
    pub user_lp_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub coin_vault: InterfaceAccount<'info, TokenAccountInterface>,

    #[account(mut)]
    pub pc_vault: InterfaceAccount<'info, TokenAccountInterface>,

    #[account(mut)]
    pub lp_mint: Account<'info, Mint>,

    pub coin_mint: InterfaceAccount<'info, MintInterface>,
    pub pc_mint: InterfaceAccount<'info, MintInterface>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub coin_token_program: Interface<'info, TokenInterface>,
    pub pc_token_program: Interface<'info, TokenInterface>,

    /// CHECK: Optional, only required if coin_mint has a transfer hook
    #[account(
        constraint = shared::validate_transfer_hook_program(
            &coin_mint,
            &coin_transfer_hook_program.to_account_info(),
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub coin_transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// CHECK: Optional, only required if pc_mint has a transfer hook
    #[account(
        constraint = shared::validate_transfer_hook_program(
            &pc_mint,
            &pc_transfer_hook_program.to_account_info(),
            &pool.whitelisted_transfer_hooks,
            pool.num_whitelisted_hooks
        ) @ TradiumError::InvalidTransferHookProgram
    )]
    pub pc_transfer_hook_program: Option<UncheckedAccount<'info>>,
}

pub fn deposit(ctx: Context<Deposit>, amount_coin: u64, amount_pc: u64) -> Result<()> {
    let pool = &mut ctx.accounts.pool;

    // Validate input amounts
    require!(
        amount_coin > 0 || amount_pc > 0,
        TradiumError::InvalidDepositAmount
    );

    // Validate token programs match pool configuration
    require!(
        ctx.accounts.coin_token_program.key() == pool.coin_token_program,
        TradiumError::InvalidCoinTokenProgram
    );
    require!(
        ctx.accounts.pc_token_program.key() == pool.pc_token_program,
        TradiumError::InvalidPcTokenProgram
    );

    // Validate vaults and mints match pool configuration
    require!(
        ctx.accounts.coin_vault.key() == pool.coin_vault,
        TradiumError::InvalidCoinVault
    );
    require!(
        ctx.accounts.pc_vault.key() == pool.pc_vault,
        TradiumError::InvalidPcVault
    );
    require!(
        ctx.accounts.lp_mint.key() == pool.lp_mint,
        TradiumError::InvalidLpMint
    );
    require!(
        ctx.accounts.coin_mint.key() == pool.coin_vault_mint,
        TradiumError::InvalidCoinMint
    );
    require!(
        ctx.accounts.pc_mint.key() == pool.pc_vault_mint,
        TradiumError::InvalidPcMint
    );

    // Get current vault balances before deposit
    let coin_vault_balance_before = ctx.accounts.coin_vault.amount;
    let pc_vault_balance_before = ctx.accounts.pc_vault.amount;
    let total_lp_supply = ctx.accounts.lp_mint.supply;

    // Transfer coin tokens from user to vault if amount > 0
    if amount_coin > 0 {
        shared::transfer_tokens_with_hook_support(
            &ctx.accounts.coin_token_program,
            &ctx.accounts.user_coin_account,
            &ctx.accounts.coin_vault,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.coin_mint,
            ctx.accounts.coin_transfer_hook_program.as_ref(),
            amount_coin,
            None,
        )?;
    }

    // Transfer PC tokens from user to vault if amount > 0
    if amount_pc > 0 {
        shared::transfer_tokens_with_hook_support(
            &ctx.accounts.pc_token_program,
            &ctx.accounts.user_pc_account,
            &ctx.accounts.pc_vault,
            &ctx.accounts.user.to_account_info(),
            &ctx.accounts.pc_mint,
            ctx.accounts.pc_transfer_hook_program.as_ref(),
            amount_pc,
            None,
        )?;
    }

    // Calculate LP tokens to mint
    let lp_amount = calculate_lp_tokens(
        pool,
        amount_coin,
        amount_pc,
        coin_vault_balance_before,
        pc_vault_balance_before,
        total_lp_supply,
    )?;

    require!(lp_amount > 0, TradiumError::InsufficientLiquidityMinted);

    // Create mint authority seeds for PDA signing
    let mint_authority_bump = pool.nonce[0];
    let pool_key = pool.key();
    let mint_authority_seeds: &[&[u8]] =
        &[b"mint_authority", pool_key.as_ref(), &[mint_authority_bump]];
    let signer_seeds: &[&[&[u8]]] = &[mint_authority_seeds];

    // Mint LP tokens to user
    let mint_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        MintTo {
            mint: ctx.accounts.lp_mint.to_account_info(),
            to: ctx.accounts.user_lp_account.to_account_info(),
            authority: pool.to_account_info(),
        },
        signer_seeds,
    );

    token::mint_to(mint_ctx, lp_amount)?;

    // Update pool state
    pool.lp_amount = pool
        .lp_amount
        .checked_add(lp_amount)
        .ok_or(TradiumError::MathOverflow)?;

    // Update nonce for security
    pool.nonce[0] = pool.nonce[0]
        .checked_add(1)
        .ok_or(TradiumError::MathOverflow)?;

    msg!(
        "Deposited {} coin tokens, {} pc tokens, minted {} LP tokens",
        amount_coin,
        amount_pc,
        lp_amount
    );

    Ok(())
}

fn calculate_lp_tokens(
    pool: &Tradium,
    amount_coin: u64,
    amount_pc: u64,
    coin_vault_balance_before: u64,
    pc_vault_balance_before: u64,
    total_lp_supply: u64,
) -> Result<u64> {
    let lp_amount = if total_lp_supply == 0 {
        let coin_amount_normalized =
            normalize_amount(amount_coin, pool.coin_decimals, pool.sys_decimal_value)?;
        let pc_amount_normalized =
            normalize_amount(amount_pc, pool.pc_decimals, pool.sys_decimal_value)?;

        // Calculate geometric mean: sqrt(coin_normalized * pc_normalized)
        integer_sqrt(
            coin_amount_normalized
                .checked_mul(pc_amount_normalized)
                .ok_or(TradiumError::MathOverflow)?,
        )?
    } else {
        // Subsequent deposits - maintain proportional shares
        let coin_share = if coin_vault_balance_before > 0 && amount_coin > 0 {
            amount_coin
                .checked_mul(total_lp_supply)
                .ok_or(TradiumError::MathOverflow)?
                .checked_div(coin_vault_balance_before)
                .ok_or(TradiumError::MathOverflow)?
        } else {
            0
        };

        let pc_share = if pc_vault_balance_before > 0 && amount_pc > 0 {
            amount_pc
                .checked_mul(total_lp_supply)
                .ok_or(TradiumError::MathOverflow)?
                .checked_div(pc_vault_balance_before)
                .ok_or(TradiumError::MathOverflow)?
        } else {
            0
        };

        // Use the minimum of the two shares to prevent dilution attacks
        std::cmp::min(coin_share, pc_share)
    };

    Ok(lp_amount)
}

fn normalize_amount(amount: u64, token_decimals: u64, sys_decimals: u64) -> Result<u64> {
    if sys_decimals >= token_decimals {
        amount
            .checked_mul(10_u64.pow((sys_decimals - token_decimals) as u32))
            .ok_or(TradiumError::MathOverflow.into())
    } else {
        amount
            .checked_div(10_u64.pow((token_decimals - sys_decimals) as u32))
            .ok_or(TradiumError::MathOverflow.into())
    }
}

fn integer_sqrt(n: u64) -> Result<u64> {
    if n == 0 {
        return Ok(0);
    }

    let mut x = n;
    let mut y = (x + 1) / 2;

    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }

    Ok(x)
}
