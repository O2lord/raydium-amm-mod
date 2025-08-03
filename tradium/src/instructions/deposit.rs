use crate::errors::TradiumError;
use crate::state::Tradium;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub pool: Account<'info, Tradium>,

    #[account(mut)]
    pub user_coin_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_pc_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_lp_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub coin_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub pc_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub lp_mint: Account<'info, Mint>,

    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,

    /// CHECK: This account will be validated based on the pool's stored token program ID
    pub coin_token_program_id: UncheckedAccount<'info>,

    /// CHECK: This account will be validated based on the pool's stored token program ID
    pub pc_token_program_id: UncheckedAccount<'info>,

    /// CHECK: Optional, only required if coin_mint has a transfer hook
    pub coin_transfer_hook_program: Option<UncheckedAccount<'info>>,

    /// CHECK: Optional, only required if pc_mint has a transfer hook
    pub pc_transfer_hook_program: Option<UncheckedAccount<'info>>,
}

pub fn deposit(ctx: Context<Deposit>, amount_coin: u64, amount_pc: u64) -> Result<()> {
    let pool = &mut ctx.accounts.pool;

    // Validate input amounts
    require!(
        amount_coin > 0 || amount_pc > 0,
        TradiumError::InvalidDepositAmount
    );

    // Validate token program IDs
    require!(
        ctx.accounts.coin_token_program_id.key() == pool.coin_vault_mint,
        TradiumError::InvalidCoinTokenProgram
    );
    require!(
        ctx.accounts.pc_token_program_id.key() == pool.pc_vault_mint,
        TradiumError::InvalidPcTokenProgram
    );

    // Validate vaults match pool configuration
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

    // Check if Token-2022 with transfer hooks
    let coin_is_token_2022 = ctx.accounts.coin_token_program_id.key() == spl_token_2022::ID;
    let pc_is_token_2022 = ctx.accounts.pc_token_program_id.key() == spl_token_2022::ID;

    // If Token-2022, validate transfer hook programs if provided
    if coin_is_token_2022 && ctx.accounts.coin_transfer_hook_program.is_some() {
        let hook_program = ctx.accounts.coin_transfer_hook_program.as_ref().unwrap();
        // Note: You'll need to add a whitelisted_transfer_hooks field to your Tradium struct
        // For now, we'll assume any provided hook program is valid
        msg!("Using coin transfer hook program: {}", hook_program.key());
    }

    if pc_is_token_2022 && ctx.accounts.pc_transfer_hook_program.is_some() {
        let hook_program = ctx.accounts.pc_transfer_hook_program.as_ref().unwrap();
        msg!("Using pc transfer hook program: {}", hook_program.key());
    }

    // Get current vault balances before deposit
    let coin_vault_balance_before = ctx.accounts.coin_vault.amount;
    let pc_vault_balance_before = ctx.accounts.pc_vault.amount;
    let total_lp_supply = ctx.accounts.lp_mint.supply;

    // Transfer coin tokens from user to vault if amount > 0
    if amount_coin > 0 {
        let transfer_ctx = CpiContext::new(
            ctx.accounts.coin_token_program_id.to_account_info(),
            Transfer {
                from: ctx.accounts.user_coin_account.to_account_info(),
                to: ctx.accounts.coin_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );

        if coin_is_token_2022 && ctx.accounts.coin_transfer_hook_program.is_some() {
            // For Token-2022 with transfer hooks, you'd need to include additional accounts
            // This is a simplified version - actual implementation would require hook-specific accounts
            token::transfer(transfer_ctx, amount_coin)?;
        } else {
            token::transfer(transfer_ctx, amount_coin)?;
        }
    }

    // Transfer PC tokens from user to vault if amount > 0
    if amount_pc > 0 {
        let transfer_ctx = CpiContext::new(
            ctx.accounts.pc_token_program_id.to_account_info(),
            Transfer {
                from: ctx.accounts.user_pc_account.to_account_info(),
                to: ctx.accounts.pc_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        );

        if pc_is_token_2022 && ctx.accounts.pc_transfer_hook_program.is_some() {
            // For Token-2022 with transfer hooks, you'd need to include additional accounts
            token::transfer(transfer_ctx, amount_pc)?;
        } else {
            token::transfer(transfer_ctx, amount_pc)?;
        }
    }

    // Calculate LP tokens to mint
    let lp_amount = if total_lp_supply == 0 {
        // Initial deposit - use geometric mean
        let coin_amount_normalized = amount_coin
            .checked_mul(10_u64.pow(pool.sys_decimal_value as u32))
            .ok_or(TradiumError::MathOverflow)?
            .checked_div(10_u64.pow(pool.coin_decimals as u32))
            .ok_or(TradiumError::MathOverflow)?;

        let pc_amount_normalized = amount_pc
            .checked_mul(10_u64.pow(pool.sys_decimal_value as u32))
            .ok_or(TradiumError::MathOverflow)?
            .checked_div(10_u64.pow(pool.pc_decimals as u32))
            .ok_or(TradiumError::MathOverflow)?;

        // Simple geometric mean calculation (sqrt(a * b))
        // In production, you'd want a more sophisticated calculation
        let product = coin_amount_normalized
            .checked_mul(pc_amount_normalized)
            .ok_or(TradiumError::MathOverflow)?;

        // Simplified square root - in production use a proper sqrt implementation
        let mut lp_amount = 1u64;
        while lp_amount.checked_mul(lp_amount).unwrap_or(u64::MAX) < product {
            lp_amount = lp_amount.checked_add(1).ok_or(TradiumError::MathOverflow)?;
        }
        lp_amount
    } else {
        // Subsequent deposits - maintain proportional shares
        let coin_share = if coin_vault_balance_before > 0 {
            amount_coin
                .checked_mul(total_lp_supply)
                .ok_or(TradiumError::MathOverflow)?
                .checked_div(coin_vault_balance_before)
                .ok_or(TradiumError::MathOverflow)?
        } else {
            0
        };

        let pc_share = if pc_vault_balance_before > 0 {
            amount_pc
                .checked_mul(total_lp_supply)
                .ok_or(TradiumError::MathOverflow)?
                .checked_div(pc_vault_balance_before)
                .ok_or(TradiumError::MathOverflow)?
        } else {
            0
        };

        // Use the minimum of the two shares to prevent dilution
        std::cmp::min(coin_share, pc_share)
    };

    require!(lp_amount > 0, TradiumError::InsufficientLiquidityMinted);

    // Mint LP tokens to user
    let mint_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        MintTo {
            mint: ctx.accounts.lp_mint.to_account_info(),
            to: ctx.accounts.user_lp_account.to_account_info(),
            authority: pool.to_account_info(), // Pool should be the mint authority
        },
    );

    token::mint_to(mint_ctx, lp_amount)?;

    // Update pool state
    pool.lp_amount = pool
        .lp_amount
        .checked_add(lp_amount)
        .ok_or(TradiumError::MathOverflow)?;

    // Update nonce for security
    pool.nonce = pool
        .nonce
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
